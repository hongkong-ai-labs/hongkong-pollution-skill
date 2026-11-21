# The release jobs copy a static musl binary in here; nothing else is needed.
FROM scratch

COPY hkpollution /hkpollution
COPY LICENSE /LICENSE

ENTRYPOINT ["/hkpollution"]
