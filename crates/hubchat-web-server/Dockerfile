# syntax=docker/dockerfile:1
# Hubchat Web's image: ONE file, the statically linked server with the web
# app inside it (docs/hubchat-web-server.md). No shell, no libraries, no
# volume, no user data.
#
# The build context is a folder that holds the built program as
# `hubchat-web` (x86_64-unknown-linux-musl; .github/workflows/build-web.yml
# makes it, with HUBCHAT_WEB_DIST set to the built page):
#
#   docker build -f crates/hubchat-web-server/Dockerfile -t hubchat-web <folder with hubchat-web>
FROM scratch
# Set by the release workflow; a local build leaves them empty. image.source
# links the package ghcr.io/maurdekye/hubchat-web to the repository.
ARG HUBCHAT_WEB_IMAGE_VERSION=
ARG HUBCHAT_WEB_IMAGE_REVISION=
LABEL org.opencontainers.image.title="hubchat-web" \
      org.opencontainers.image.description="Hubchat Web: serves Hubchat as a web app, for an iPhone or any browser. It only hands out the app; identities, keys and messages stay in each person's browser." \
      org.opencontainers.image.source="https://github.com/Maurdekye/orgtree-hubchat" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${HUBCHAT_WEB_IMAGE_VERSION}" \
      org.opencontainers.image.revision="${HUBCHAT_WEB_IMAGE_REVISION}"
COPY --chmod=0555 hubchat-web /hubchat-web
# not root, and it needs to write nothing: run it with a read-only file system
USER 65532:65532
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD ["/hubchat-web", "healthcheck"]
ENTRYPOINT ["/hubchat-web"]
