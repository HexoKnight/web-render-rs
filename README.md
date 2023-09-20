# web-render-rs
a Rust crate for rendering/gamedev using wasm and webgl

# Examples
to build the `web-test` example you'll want `waasm-pack`, and some local http server, eg. `live-server`:
```
cargo install wasm-pack
cargo install live-server
```

```
cd examples/web-test
wasm-pack build --target web
live-server -h localhost -p 8000
```
then visit `localhost:8000` and voilá
