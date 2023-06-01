#!/bin/bash
set -x

ARG=$1

if [ "$ARG" = "--clearnet" ]; then
	NETWORK=clearnet
elif [ "$ARG" = "--tor" ]; then
	NETWORK=tor
	ADDITIONAL_ARGS=--cap-add=NET_ADMIN
else
	echo "Usage $0 --clearnet|tor";
	exit 1
fi

echo $NETWORK

docker build --build-arg NETWORK=$NETWORK -t dostrnip5v1 . && \
	docker run --restart=unless-stopped -p 3033:3033 -ti --name=dostrnip5v1 -v "$(pwd)/data:/app/data" -v "$(pwd)/web:/app/web" dostrnip5v1
