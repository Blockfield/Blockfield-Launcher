FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y xorg-x11-server-Xvfb mesa-dri-drivers mesa-libGL mesa-libEGL fontconfig libxkbcommon-x11 libXcursor libXi libXrandr libXinerama libX11-xcb python3 ImageMagick ffmpeg-free && dnf clean all
ENV DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe SLINT_BACKEND=winit-skia-opengl WINIT_UNIX_BACKEND=x11
