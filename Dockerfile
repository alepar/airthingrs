FROM debian:bullseye AS builder

RUN apt update
RUN apt install -y \
    curl \
    build-essential \
    libdbus-1-dev

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN mkdir -p /app/src
COPY . /app/src
WORKDIR /app/src
RUN cargo build --release

FROM debian:bullseye
RUN apt update
RUN apt install -y \
    dbus

RUN mkdir -p /app/bin
COPY --from=builder /app/src/target/release/wavething-rust /app/bin/wavething-rust
COPY --from=builder /app/src/devices.toml /app/bin/devices.toml
WORKDIR /app/bin
