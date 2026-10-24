//! Slint skin preview host: worker-thread wgpu rendering with strict pacing.
//! See SLINT.md for the ownership and pacing contract. The UI thread owns all
//! Slint objects, the frame deadline timer and the active animation clock; the
//! worker owns wgpu, texture decoding, scene rendering and synchronous GPU
//! readback. `SharedPixelBuffer` crosses back through `invoke_from_event_loop`.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};

use super::slint_pacing::{ActiveClock, next_deadline, timer_delay};
use super::{Cape, Model, RenderSize, Renderer, Skin};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode};

pub const PREVIEW_WIDTH: u32 = 220;
pub const PREVIEW_HEIGHT: u32 = 300;
const PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Pose clock: live wall time, a frozen pose, or deterministic delivered-index time.
#[derive(Clone, Copy)]
pub enum ClockMode {
    Live(f64),
    Fixed(f64),
    Indexed(f64),
}

/// UI-thread callbacks into the concrete Slint window. `notify_frame` and
/// `poll_window` must only touch Slint objects on the event-loop thread; the
/// worker only invokes `notify_frame` through `invoke_from_event_loop`.
pub struct FrameSink {
    pub set_frame: Box<dyn Fn(Image)>,
    pub set_error: Box<dyn Fn(SharedString)>,
    pub notify_frame: Arc<dyn Fn() + Send + Sync>,
    pub poll_window: Box<dyn Fn() -> (bool, f32)>,
}

/// Called on the event-loop thread after each delivered frame as (index, time).
pub type FrameHook = Box<dyn Fn(u64, f64)>;

pub struct HostOptions {
    pub clock: ClockMode,
    /// Replacable after spawn for binaries that need the host handle inside hooks.
    pub frame_hook: Option<FrameHook>,
    pub sink: FrameSink,
}

enum Request {
    Render {
        time: f64,
        scale: f32,
        generation: u64,
    },
    DragState(f64, bool),
    DragBy(f32, f32),
    Stop,
}

pub struct PreviewHost {
    sender: Sender<Request>,
    frames: mpsc::Receiver<Delivery>,
    worker: RefCell<Option<std::thread::JoinHandle<()>>>,
    timer: Timer,
    deadline: Cell<Instant>,
    clock: RefCell<ActiveClock>,
    clock_mode: ClockMode,
    frame_hook: RefCell<Option<FrameHook>>,
    on_ready: RefCell<Option<Box<dyn Fn()>>>,
    sink: FrameSink,
    enabled: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    tab_visible: Cell<bool>,
    window_live: Cell<bool>,
    scale: Cell<f32>,
    generation: Cell<u64>,
    failed: Cell<bool>,
    delivered: Cell<u64>,
    drag_moves: Cell<u32>,
}

type Delivery = (u64, f64, Result<SharedPixelBuffer<Rgba8Pixel>, String>);

impl PreviewHost {
    pub fn spawn(options: HostOptions) -> std::io::Result<Rc<Self>> {
        let (sender, receiver) = mpsc::channel();
        let (output, frames) = mpsc::channel();
        let host = Rc::new(Self {
            sender,
            frames,
            worker: RefCell::new(None),
            timer: Timer::default(),
            deadline: Cell::new(Instant::now()),
            clock: RefCell::default(),
            clock_mode: options.clock,
            frame_hook: RefCell::new(options.frame_hook),
            on_ready: RefCell::new(None),
            sink: options.sink,
            enabled: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
            tab_visible: Cell::new(false),
            window_live: Cell::new(false),
            scale: Cell::new(1.0),
            generation: Cell::new(0),
            failed: Cell::new(false),
            delivered: Cell::new(0),
            drag_moves: Cell::new(0),
        });
        let enabled = host.enabled.clone();
        let busy = host.busy.clone();
        let notify = host.sink.notify_frame.clone();
        let worker = std::thread::Builder::new()
            .name("skin3d-render".into())
            .spawn(move || render_worker(receiver, output, enabled, busy, notify))?;
        host.worker.replace(Some(worker));
        Ok(host)
    }

    pub fn shutdown(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.timer.stop();
        let _ = self.sender.send(Request::Stop);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            eprintln!("Skin worker panicked");
        }
    }

    fn time(&self) -> f64 {
        match self.clock_mode {
            ClockMode::Live(start) => start + self.clock.borrow().seconds(Instant::now()),
            ClockMode::Fixed(time) => time,
            ClockMode::Indexed(start) => start + self.delivered.get() as f64 / 60.0,
        }
    }

    /// Recompute visibility from the latest tab and window state. The binary
    /// calls this after any visibility change and before each frame request so
    /// a minimized window stops even when the OS sends no occlusion event.
    pub fn refresh(self: &Rc<Self>) {
        let (live, scale) = (self.sink.poll_window)();
        self.window_live.set(live);
        self.scale.set(scale);
        let running = self.tab_visible.get() && self.window_live.get() && !self.failed.get();
        if self.enabled.swap(running, Ordering::SeqCst) == running {
            return;
        }
        self.generation.set(self.generation.get() + 1);
        self.clock.borrow_mut().set_running(running, Instant::now());
        if running {
            self.deadline.set(Instant::now() + PERIOD);
            self.request_frame();
            self.schedule_frame();
        } else {
            self.timer.stop();
            let _ = self.sender.send(Request::DragState(self.time(), false));
        }
        println!(
            "PACING running={running} delivered={} time={:.6}",
            self.delivered.get(),
            self.time()
        );
    }

    pub fn set_tab_visible(self: &Rc<Self>, visible: bool) {
        self.tab_visible.set(visible);
        self.refresh();
    }

    fn schedule_frame(self: &Rc<Self>) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let weak = Rc::downgrade(self);
        self.timer.start(
            TimerMode::SingleShot,
            timer_delay(self.deadline.get(), Instant::now()),
            move || {
                if let Some(host) = weak.upgrade() {
                    let now = Instant::now();
                    if now >= host.deadline.get() {
                        host.deadline
                            .set(next_deadline(host.deadline.get(), now, PERIOD));
                        host.poll_frame();
                        host.request_frame();
                    }
                    host.schedule_frame();
                }
            },
        );
    }

    fn request_frame(self: &Rc<Self>) {
        self.refresh();
        if !self.enabled.load(Ordering::SeqCst) || self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        if self
            .sender
            .send(Request::Render {
                time: self.time(),
                scale: self.scale.get(),
                generation: self.generation.get(),
            })
            .is_err()
        {
            self.busy.store(false, Ordering::SeqCst);
            self.failed.set(true);
            self.timer.stop();
            (self.sink.set_error)("Skin renderer stopped".into());
        }
    }

    /// Deliver one worker result. Called from the binary's frame-ready handler.
    pub fn poll_frame(self: &Rc<Self>) {
        let Ok((generation, time, result)) = self.frames.try_recv() else {
            return;
        };
        self.busy.store(false, Ordering::SeqCst);
        if generation != self.generation.get() || !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        match result {
            Ok(pixels) => {
                (self.sink.set_frame)(Image::from_rgba8(pixels));
                let index = self.delivered.get();
                self.delivered.set(index + 1);
                if index == 0 {
                    println!("READY pid={} time={time:.6}", std::process::id());
                    if let Some(ready) = self.on_ready.borrow().as_ref() {
                        ready();
                    }
                }
                if let Some(hook) = self.frame_hook.borrow().as_ref() {
                    hook(index, time);
                }
            }
            Err(error) => {
                eprintln!("Skin renderer: {error}");
                (self.sink.set_error)(error.into());
                self.failed.set(true);
                self.refresh();
            }
        }
    }

    pub fn on_drag_state(&self, active: bool) {
        let _ = self.sender.send(Request::DragState(self.time(), active));
    }

    pub fn on_drag_by(&self, dx: f32, dy: f32) {
        self.drag_moves.set(self.drag_moves.get() + 1);
        let _ = self.sender.send(Request::DragBy(dx, dy));
    }

    pub fn set_on_ready(&self, ready: impl Fn() + 'static) {
        self.on_ready.replace(Some(Box::new(ready)));
    }

    pub fn delivered(&self) -> u64 {
        self.delivered.get()
    }

    pub fn active_time(&self) -> f64 {
        self.time()
    }

    pub fn timer_running(&self) -> bool {
        self.timer.running()
    }

    pub fn drag_moves(&self) -> u32 {
        self.drag_moves.get()
    }

    pub fn failed(&self) -> bool {
        self.failed.get()
    }
}

fn render_worker(
    receiver: mpsc::Receiver<Request>,
    output: Sender<Delivery>,
    enabled: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    notify: Arc<dyn Fn() + Send + Sync>,
) {
    let mut renderer: Option<Renderer> = None;
    let mut renderer_scale = 0.0;
    while let Ok(request) = receiver.recv() {
        match request {
            Request::Stop => break,
            Request::DragState(time, active) => {
                if let Some(renderer) = &mut renderer
                    && let Err(error) = renderer.controls.set_dragging(time, active)
                {
                    eprintln!("Drag: {error}");
                }
            }
            Request::DragBy(dx, dy) => {
                if let Some(renderer) = &mut renderer
                    && let Err(error) = renderer.controls.drag_by(dx, dy, 300.0)
                {
                    eprintln!("Drag: {error}");
                }
            }
            Request::Render {
                time,
                scale,
                generation,
            } => {
                if !enabled.load(Ordering::SeqCst) {
                    busy.store(false, Ordering::SeqCst);
                    continue;
                }
                let result = (|| -> super::Result<_> {
                    if renderer.is_none() || renderer_scale != scale {
                        let skin = Skin::from_png(
                            include_bytes!("../../../src/lib/fixtures/skin.png"),
                            Model::Classic,
                        )?;
                        let cape =
                            Cape::from_png(include_bytes!("../../../src/lib/fixtures/cape.png"))?;
                        let mut replacement = pollster::block_on(Renderer::new(
                            RenderSize {
                                width: PREVIEW_WIDTH,
                                height: PREVIEW_HEIGHT,
                                scale_factor: scale,
                            },
                            &skin,
                            Some(&cape),
                            false,
                        ))?;
                        if let Some(old) = &renderer {
                            replacement.controls = old.controls.clone();
                        }
                        println!("ADAPTER {:?}", replacement.adapter_info());
                        renderer = Some(replacement);
                        renderer_scale = scale;
                    }
                    let frame = renderer.as_mut().unwrap().render_frame(time, 0.0)?;
                    Ok(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                        &frame.rgba,
                        frame.width,
                        frame.height,
                    ))
                })()
                .map_err(|error| error.to_string());
                if !enabled.load(Ordering::SeqCst) {
                    busy.store(false, Ordering::SeqCst);
                    continue;
                }
                if output.send((generation, time, result)).is_err() {
                    break;
                }
                let pending = notify.clone();
                if slint::invoke_from_event_loop(move || pending()).is_err() {
                    break;
                }
            }
        }
    }
}
