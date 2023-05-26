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

docker build --build-arg NETWORK=$NETWORK -t dostr . && \
	docker run --rm -ti --name=dostr -v$PWD/data:/app/data:rw $ADDITIONAL_ARGS dostr
