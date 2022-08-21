FROM rust:1.62-bullseye
ARG CODENAME=bullseye
# TODO: Specific commits for used repos, don't just use master HEAD

# Prevent being stuck at timezone selection
ENV TZ=Europe/London
RUN ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && \
    echo $TZ > /etc/timezone && \
    apt update && \
    apt install -y gpg wget vim git g++ python3 python3-pip expect-dev apt-transport-https iptables

# Setup tor https://support.torproject.org/apt/tor-deb-repo/
RUN echo "deb [signed-by=/usr/share/keyrings/tor-archive-keyring.gpg] https://deb.torproject.org/torproject.org ${CODENAME} main\ndeb-src [signed-by=/usr/share/keyrings/tor-archive-keyring.gpg] https://deb.torproject.org/torproject.org ${CODENAME} main" > /etc/apt/sources.list.d/tor.list && \
    wget -qO- https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc \
    | gpg --dearmor \
    | tee /usr/share/keyrings/tor-archive-keyring.gpg >/dev/null && \
    apt update && \
    apt install -y tor deb.torproject.org-keyring

ARG NETWORK

COPY config /app/
COPY startup_clearnet.sh startup_tor.sh /
RUN if [ "$NETWORK" = "clearnet" ]; then ln -s /startup_clearnet.sh /startup.sh; elif [ "$NETWORK" = "tor" ]; then ln -s /startup_tor.sh /startup.sh; else exit 1; fi

# TODO: Add non-root user and use it
ENV RUST_LOG=debug

COPY Cargo.toml /app/
COPY src /app/src

RUN cd app && cargo build --release


# Use unbuffer to preserve colors in terminal output while using tee
CMD cd /app && unbuffer /startup.sh
