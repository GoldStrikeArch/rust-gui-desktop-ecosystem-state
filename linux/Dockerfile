# Linux reality-check environment for the RCN GUI research (iteration 5).
# arm64 (Apple Silicon native). X11 headless via Xvfb; GPU = mesa lavapipe
# (software Vulkan) + llvmpipe (software GL); WebKitGTK 4.1 for the webview
# frameworks; Noto fonts (core+CJK+color emoji) for the Babel text tests.
FROM rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    # headless X11 + tooling
    xvfb xauth x11-utils x11-apps imagemagick procps \
    # dbus (menus/tray/notifications plumbing)
    dbus dbus-x11 \
    # software GPU: Vulkan (lavapipe) + GL (llvmpipe)
    mesa-vulkan-drivers vulkan-tools libvulkan1 libgl1-mesa-dri libegl1 libgl1 \
    # windowing/input libs (winit X11 path + xkb)
    libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxcb1 \
    libxcb-render0 libxcb-shape0 libxcb-xfixes0 libxcb-xkb1 libxcb-icccm4 \
    libxcb-cursor0 libxrandr2 libxi6 libxcursor1 libwayland-client0 \
    # GTK + WebKitGTK for tauri/dioxus (and the tray/menu fault-line tests)
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
    libxdo-dev \
    # build essentials
    libssl-dev pkg-config cmake clang \
    # fonts for the Babel corpus
    fontconfig fonts-noto-core fonts-noto-cjk fonts-noto-color-emoji \
    fonts-noto-extra \
    # audio stack presence (some apps link ALSA transitively)
    libasound2-dev \
    && rm -rf /var/lib/apt/lists/*

# Record environment versions for the evidence manifest
RUN mkdir -p /env && \
    { echo "rustc: $(rustc --version)"; \
      echo "webkit2gtk: $(dpkg -s libwebkit2gtk-4.1-0 2>/dev/null | grep '^Version' || true)"; \
      echo "gtk3: $(dpkg -s libgtk-3-0 2>/dev/null | grep '^Version' || true)"; \
      echo "mesa: $(dpkg -s mesa-vulkan-drivers 2>/dev/null | grep '^Version' || true)"; \
      echo "debian: $(cat /etc/debian_version)"; } > /env/versions.txt

ENV DISPLAY=:99
WORKDIR /work
