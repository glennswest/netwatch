FROM scratch
COPY netwatch /netwatch
EXPOSE 8080
ENTRYPOINT ["/netwatch"]
CMD ["--config", "/etc/netwatch/netwatch.toml", "--data-dir", "/var/lib/netwatch"]
