FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ewf-tools ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["ewfexport"]