FROM registry.gt.lo:5000/stormdbase:latest

# netwatch binary (pre-built for aarch64-musl)
COPY target/aarch64-unknown-linux-musl/release/netwatch /app/netwatch

# stormd supervisor config + netwatch config
COPY deploy/stormd-config.toml /etc/stormd/config.toml
COPY deploy/netwatch.toml /etc/netwatch/netwatch.toml

VOLUME ["/data"]
EXPOSE 80 9080 22
ENTRYPOINT ["/stormd"]
CMD ["--config", "/etc/stormd/config.toml"]
