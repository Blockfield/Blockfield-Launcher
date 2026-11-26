FROM localhost/blockfield-main-headless:latest
# One Fedora Mesa stack for both apps; headers compile the capture-only preload.
RUN dnf install -y webkit2gtk4.1-devel gcc pkgconf-pkg-config dbus-x11 glx-utils && dnf clean all
COPY scripts/capture/console.c /tmp/console.c
RUN gcc -shared -fPIC /tmp/console.c -o /opt/console.so $(pkg-config --cflags --libs webkit2gtk-4.1) -ldl && rm /tmp/console.c
ENV GDK_BACKEND=x11 GDK_SCALE=1 GDK_DPI_SCALE=1 SLINT_SCALE_FACTOR=1
ENV WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
ENV TZ=UTC LANG=C.UTF-8
