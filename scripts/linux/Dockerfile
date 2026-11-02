FROM rust:1.97.1-trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libxkbcommon-dev libxkbcommon-x11-0 \
    libmpv2 xvfb xauth mesa-vulkan-drivers fonts-dejavu-core \
    ffmpeg openbox xdotool x11-utils imagemagick dbus-x11 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy

ENV CARGO_BUILD_WARNINGS=deny
WORKDIR /work
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY crates ./crates
COPY scripts/linux ./scripts/linux
CMD ["bash", "scripts/linux/test.sh"]
