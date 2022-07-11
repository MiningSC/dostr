FROM rust:1.61
# TODO: Specific commits for used repos, don't just use master HEAD

# Prevent being stuck at timezone selection
ENV TZ=Europe/London
RUN ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && echo $TZ > /etc/timezone


RUN apt update && apt install -y gpg wget vim git g++ python3 python3-pip expect-dev

# Build twint
# Also prevent crash when profile doesn't have url or banner url
RUN git clone --depth=1 https://github.com/minamotorin/twint.git && \
    cd twint && \
    pip3 install . -r requirements.txt && \
    sed -i 's/    _usr\.url =.*/    _usr\.url = ""/'  /usr/local/lib/python3.9/dist-packages/twint/user.py  && \
    sed -i 's/    _usr\.background_image =.*/    _usr\.background_image = ""/'  /usr/local/lib/python3.9/dist-packages/twint/user.py

COPY Cargo.toml /app/
COPY src /app/src

RUN cd /app && \
    cargo build --release

# TODO: Add non-root user and use it
COPY config /app/
ENV RUST_LOG=debug

# Use unbuffer to preserve colors in terminal output while using tee
CMD cd /app && unbuffer cargo run --release 2>&1 | tee -a data/log
