FROM scratch
COPY stormd /stormd
COPY netwatch /app/netwatch
COPY deploy/stormd-config.toml /etc/stormd/config.toml
COPY deploy/netwatch.toml /etc/netwatch/netwatch.toml
VOLUME ["/data"]
EXPOSE 8080 9080 22
ENTRYPOINT ["/stormd"]
CMD ["--config", "/etc/stormd/config.toml"]
