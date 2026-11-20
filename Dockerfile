FROM rust:1.97.1 AS build
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN cargo build --locked --release -p mobility-api --bin spatial-workbench

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/spatial-workbench /usr/local/bin/
COPY tests/acceptance/e20_workspace.py LICENSE /opt/carfree/
USER 65532:65532
CMD ["spatial-workbench"]
