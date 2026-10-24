#[path = "slint_preview/pacing.rs"]
mod pacing;

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};

use pacing::{ActiveClock, next_deadline, timer_delay};
use skin3d::{Cape, Model, RenderSize, Renderer, Skin};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode};

slint::slint! { export { PreviewWindow } from "slint_preview/preview.slint"; }

const PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 60);

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

struct Preview {
    window: slint::Weak<PreviewWindow>,
    sender: Sender<Request>,
    timer: Timer,
    deadline: Cell<Instant>,
    clock: RefCell<ActiveClock>,
    enabled: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    generation: Cell<u64>,
    occluded: Cell<bool>,
    nonzero_size: Cell<bool>,
    failed: Cell<bool>,
    delivered: Cell<u64>,
    drag_moves: Cell<u32>,
    capture_dir: Option<PathBuf>,
    start_time: f64,
    self_check: bool,
}

impl Preview {
    fn time(&self) -> f64 {
        if self.capture_dir.is_some() {
            self.start_time + self.delivered.get() as f64 / 60.0
        } else {
            self.start_time + self.clock.borrow().seconds(Instant::now())
        }
    }

    fn visibility(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let minimized = window
            .window()
            .with_winit_window(|window| window.is_minimized())
            .flatten()
            .unwrap_or(false);
        let running = window.get_preview_visible()
            && window.window().is_visible()
            && !self.occluded.get()
            && self.nonzero_size.get()
            && !minimized
            && !self.failed.get();
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

    fn schedule_frame(self: &Rc<Self>) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let weak = Rc::downgrade(self);
        self.timer.start(
            TimerMode::SingleShot,
            timer_delay(self.deadline.get(), Instant::now()),
            move || {
                if let Some(preview) = weak.upgrade() {
                    let now = Instant::now();
                    if now >= preview.deadline.get() {
                        preview
                            .deadline
                            .set(next_deadline(preview.deadline.get(), now, PERIOD));
                        preview.request_frame();
                    }
                    preview.schedule_frame();
                }
            },
        );
    }

    fn request_frame(self: &Rc<Self>) {
        self.visibility();
        if !self.enabled.load(Ordering::SeqCst) || self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if self
            .sender
            .send(Request::Render {
                time: self.time(),
                scale: window.window().scale_factor(),
                generation: self.generation.get(),
            })
            .is_err()
        {
            self.busy.store(false, Ordering::SeqCst);
            self.failed.set(true);
            self.timer.stop();
            window.set_error("Skin renderer stopped".into());
        }
    }

    fn frame(
        self: &Rc<Self>,
        generation: u64,
        time: f64,
        result: Result<SharedPixelBuffer<Rgba8Pixel>, String>,
    ) {
        self.busy.store(false, Ordering::SeqCst);
        if generation != self.generation.get() || !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match result {
            Ok(pixels) => {
                window.set_frame(Image::from_rgba8(pixels));
                let index = self.delivered.get();
                self.delivered.set(index + 1);
                if index == 0 {
                    println!("READY pid={} time={time:.6}", std::process::id());
                    if self.self_check {
                        self.start_self_check();
                    }
                }
                if let Some(directory) = &self.capture_dir {
                    if index.is_multiple_of(60) {
                        let snapshot = window.window().take_snapshot().expect("Slint snapshot");
                        image::save_buffer(
                            directory.join(format!("frame-{index:03}.png")),
                            snapshot.as_bytes(),
                            snapshot.width(),
                            snapshot.height(),
                            image::ColorType::Rgba8,
                        )
                        .expect("save snapshot");
                        println!("CAPTURE index={index} time={time:.6}");
                    }
                    if index == 180 {
                        slint::quit_event_loop().expect("finish capture");
                    }
                }
            }
            Err(error) => {
                eprintln!("Skin renderer: {error}");
                window.set_error(error.into());
                self.failed.set(true);
                self.visibility();
            }
        }
    }

    fn start_self_check(self: &Rc<Self>) {
        let this = self.clone();
        Timer::single_shot(Duration::from_millis(300), move || {
            let window = this.window.upgrade().unwrap();
            use slint::{
                LogicalPosition,
                platform::{PointerEventButton, WindowEvent},
            };
            for event in [
                WindowEvent::PointerPressed {
                    position: LogicalPosition::new(160., 140.),
                    button: PointerEventButton::Left,
                },
                WindowEvent::PointerMoved {
                    position: LogicalPosition::new(205., 165.),
                },
                WindowEvent::PointerReleased {
                    position: LogicalPosition::new(205., 165.),
                    button: PointerEventButton::Left,
                },
            ] {
                window.window().dispatch_event(event);
            }
            assert!(
                this.drag_moves.get() > 0,
                "Slint pointer drag reaches worker controls"
            );
            println!("SELF_CHECK drag dispatched");
            window.set_preview_visible(false);
            this.visibility();
            let count = this.delivered.get();
            let time = this.time();
            Timer::single_shot(Duration::from_millis(600), move || {
                assert_eq!(this.delivered.get(), count, "no hidden frame delivery");
                assert_eq!(this.time(), time, "hidden active time must freeze");
                assert!(!this.timer.running(), "hidden frame timer stopped");
                this.window.upgrade().unwrap().set_preview_visible(true);
                this.visibility();
                let resumed = this.clone();
                Timer::single_shot(Duration::from_millis(300), move || {
                    assert!(resumed.delivered.get() > count, "visible animation resumes");
                    println!("SELF_CHECK passed: drag, hidden timer/frame/clock, resume");
                    slint::quit_event_loop().unwrap();
                });
            });
        });
    }
}

type Delivery = (u64, f64, Result<SharedPixelBuffer<Rgba8Pixel>, String>);

fn render_worker(
    receiver: mpsc::Receiver<Request>,
    output: Sender<Delivery>,
    enabled: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
    window: slint::Weak<PreviewWindow>,
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
                let result = (|| -> skin3d::Result<_> {
                    if renderer.is_none() || renderer_scale != scale {
                        let skin = Skin::from_png(
                            include_bytes!("../../../src/lib/fixtures/skin.png"),
                            Model::Classic,
                        )?;
                        let cape =
                            Cape::from_png(include_bytes!("../../../src/lib/fixtures/cape.png"))?;
                        let mut replacement = pollster::block_on(Renderer::new(
                            RenderSize {
                                width: 220,
                                height: 300,
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
                let weak = window.clone();
                if slint::invoke_from_event_loop(move || {
                    if let Some(window) = weak.upgrade() {
                        window.invoke_frame_ready();
                    }
                })
                .is_err()
                {
                    break;
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut capture_dir = None;
    let mut hidden = false;
    let mut self_check = false;
    let mut start_time = 0.0_f64;
    let mut idle_seconds = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capture-dir" => {
                capture_dir = Some(PathBuf::from(args.next().ok_or("capture path required")?))
            }
            "--hidden" => hidden = true,
            "--self-check" => self_check = true,
            "--start-time" => start_time = args.next().ok_or("start time required")?.parse()?,
            "--idle-seconds" => {
                idle_seconds = Some(args.next().ok_or("duration required")?.parse::<u64>()?)
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    if !start_time.is_finite() || start_time < 0.0 {
        return Err("start time must be finite and nonnegative".into());
    }
    if let Some(directory) = &capture_dir {
        std::fs::create_dir_all(directory)?;
    }
    if self_check && (capture_dir.is_some() || hidden) {
        return Err("self-check requires visible live animation".into());
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    let window = PreviewWindow::new()?;
    window.set_preview_visible(!hidden);
    let (sender, receiver) = mpsc::channel();
    let (output, frames) = mpsc::channel();
    let preview = Rc::new(Preview {
        window: window.as_weak(),
        sender,
        timer: Timer::default(),
        deadline: Cell::new(Instant::now()),
        clock: RefCell::default(),
        enabled: Arc::new(AtomicBool::new(false)),
        busy: Arc::new(AtomicBool::new(false)),
        generation: Cell::new(0),
        occluded: Cell::new(false),
        nonzero_size: Cell::new(true),
        failed: Cell::new(false),
        delivered: Cell::new(0),
        drag_moves: Cell::new(0),
        capture_dir,
        start_time,
        self_check,
    });
    let enabled = preview.enabled.clone();
    let busy = preview.busy.clone();
    let weak = window.as_weak();
    let worker = std::thread::Builder::new()
        .name("skin3d-render".into())
        .spawn(move || render_worker(receiver, output, enabled, busy, weak))?;
    let this = preview.clone();
    window.on_frame_ready(move || {
        if let Ok((generation, time, result)) = frames.try_recv() {
            this.frame(generation, time, result);
        }
    });
    let this = preview.clone();
    window.on_visibility_changed(move || this.visibility());
    let this = preview.clone();
    window.on_drag_state(move |active| {
        let _ = this.sender.send(Request::DragState(this.time(), active));
    });
    let this = preview.clone();
    window.on_drag_by(move |dx, dy| {
        this.drag_moves.set(this.drag_moves.get() + 1);
        let _ = this.sender.send(Request::DragBy(dx, dy));
    });
    let this = preview.clone();
    window.window().on_winit_window_event(move |_, event| {
        match event {
            WindowEvent::Occluded(occluded) => this.occluded.set(*occluded),
            WindowEvent::Resized(size) => this.nonzero_size.set(size.width > 0 && size.height > 0),
            WindowEvent::Focused(false) => {
                let _ = this.sender.send(Request::DragState(this.time(), false));
            }
            _ => {}
        }
        this.visibility();
        EventResult::Propagate
    });
    if let Some(seconds) = idle_seconds {
        Timer::single_shot(Duration::from_secs(seconds), || {
            slint::quit_event_loop().unwrap();
        });
    }
    window.show()?;
    preview.visibility();
    if hidden {
        println!("READY pid={} hidden=true", std::process::id());
    }
    let outcome = slint::run_event_loop();
    preview.enabled.store(false, Ordering::SeqCst);
    preview.timer.stop();
    let _ = preview.sender.send(Request::Stop);
    worker.join().map_err(|_| "skin worker panicked")?;
    outcome?;
    if preview.failed.get() {
        return Err("skin rendering failed".into());
    }
    Ok(())
}
