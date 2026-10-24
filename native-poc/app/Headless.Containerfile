FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y xorg-x11-server-Xvfb mesa-dri-drivers mesa-libGL mesa-libEGL fontconfig libxkbcommon-x11 libXcursor libXi libXrandr libXinerama libX11-xcb python3 ImageMagick ffmpeg-free && dnf clean all
ENV DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe SLINT_BACKEND=winit-skia-opengl WINIT_UNIX_BACKEND=x11
# Skia GL redraws the whole window every animated frame (u10: ~3.8 cores llvmpipe);
# partial rendering keeps identical pixels (AE=0 on all main captures) at ~0.2 cores.
ENV SLINT_SKIA_PARTIAL_RENDERING=1
