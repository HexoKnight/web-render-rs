use wasm_bindgen::prelude::*;
use web_sys::WebGl2RenderingContext;
extern crate console_error_panic_hook;
extern crate web_render_rs;
use web_render_rs::{Renderer, UpdateInfo, RenderInfo};

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;

    let renderer = Renderer::from_canvas(canvas)?
        .with_on_resize(|_state, (x, y)| {
            web_sys::console::log_3(&"canvas size: ".into(), &x.into(), &y.into());
            // could use to lower resolution:
            (x/*  / 10 */, y/*  / 10 */)
        }).unwrap()
        .with_on_render(on_render).unwrap()
        .with_on_update(on_update).unwrap()
        .with_shaders(include_str!("vert_shader.glsl"), include_str!("frag_shader.glsl")).unwrap()
        .with_on_event("keydown", on_keydown)?
        .with_on_event("click", on_click)?;

    let state = State {
        x: 1.0,
        y: 0.0,
        most_recent_key: String::new(),
    };
    renderer.start(state, 60, 0.1);
    Ok(())
}

struct State {
    pub x: f32,
    pub y: f32,
    pub most_recent_key: String,
}

fn on_keydown(state: &mut State, event: web_sys::Event) {
    let event = event.dyn_into::<web_sys::KeyboardEvent>().unwrap();
    web_sys::console::log_2(&"key: ".into(), &event.key().into());
    state.most_recent_key = event.key();
}

fn on_click(state: &mut State, event: web_sys::Event) {
    let event = event.dyn_into::<web_sys::MouseEvent>().unwrap();
    web_sys::console::log_3(&"screen pos: ".into(), &event.client_x().into(), &event.client_y().into());
    web_sys::console::log_3(&"canvas pos: ".into(), &event.offset_x().into(), &event.offset_y().into());
}

fn on_update(update_info: UpdateInfo<State>) {
    update_info.state.x = update_info.state.x + 0.01;
    if update_info.state.x > 1.0 {
        update_info.state.x -= 2.0;
    }
    update_info.state.y = update_info.state.y + 0.02;
    if update_info.state.y > 1.0 {
        update_info.state.y -= 2.0;
    }
}

fn on_render(render_info: RenderInfo<State>) {
    let context = render_info.context();

    let vertices: [f32; 9] = [render_info.state.x, render_info.state.y, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0];

    let buffer = context.create_buffer().unwrap();
    context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));

    // Note that `Float32Array::view` is somewhat dangerous (hence the
    // `unsafe`!). This is creating a raw view into our module's
    // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
    // (aka do a memory allocation in Rust) it'll cause the buffer to change,
    // causing the `Float32Array` to be invalid.
    //
    // As a result, after `Float32Array::view` we have to be very careful not to
    // do any memory allocations before it's dropped.
    unsafe {
        let vert_array = js_sys::Float32Array::view(&vertices);

        context.buffer_data_with_array_buffer_view(
            WebGl2RenderingContext::ARRAY_BUFFER,
            &vert_array,
            WebGl2RenderingContext::STATIC_DRAW,
        );
    }

    context.vertex_attrib_pointer_with_i32(0, 3, WebGl2RenderingContext::FLOAT, false, 0, 0);
    context.enable_vertex_attrib_array(0);

    context.clear_color(0.0, 0.0, 0.0, 1.0);
    context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

    context.draw_arrays(
        WebGl2RenderingContext::TRIANGLES,
        0,
        (vertices.len() / 3) as i32,
    );
}