#!/bin/bash
set -x
set -e

cd /app && unbuffer ./target/release/dostr --clearnet | tee -a data/log
