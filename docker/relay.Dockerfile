# Public UDP relay (docs/architecture/08-infrastructure.md). Built by
# .github/workflows/release-artifacts.yml from the static musl binaries; no compiler here.
#
#   docker run -d -p 7400:7400/udp -v lattice-relay:/data ghcr.io/idnwit/lattice-relay:latest
#
# The relay key lives in /data/relay.key, created on first start. Keep the volume:
# a new key changes the relay NodeId that every paired node pins.
FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETARCH
COPY --chown=nonroot:nonroot data /data
COPY ${TARGETARCH}/lattice-relay /usr/local/bin/lattice-relay
ENV RUST_LOG=info
EXPOSE 7400/udp
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/lattice-relay", "--key", "/data/relay.key", "--listen", "0.0.0.0:7400"]
