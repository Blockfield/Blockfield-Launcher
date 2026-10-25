FROM localhost/blockfield-native-headless:latest
RUN dnf install -y mesa-vulkan-drivers vulkan-loader ffmpeg-free xdotool openbox && dnf clean all
ENV LP_NUM_THREADS=4
