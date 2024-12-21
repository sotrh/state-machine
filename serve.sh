#!/bin/sh -e
# https://github.com/erer1243/wgpu-0.20-winit-0.30-web-example/blob/master/web.sh
wasm-pack build --no-typescript --no-pack --target=web --dev
python3 -m http.server