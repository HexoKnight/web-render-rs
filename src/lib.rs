use wasm_bindgen::{JsValue, JsCast, closure::Closure};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext, WebGlProgram, WebGlShader, Event, window};
use std::cell::{OnceCell, RefCell};
use std::ops::DerefMut;
use std::rc::Rc;

pub struct Renderer<S>
    where S: 'static
{
    canvas: Rc<HtmlCanvasElement>,
    context: Rc<WebGl2RenderingContext>,
    state: Rc<OnceCell<RefCell<S>>>,

    on_update: OnceCell<fn(UpdateInfo<S>)>,
    on_render: OnceCell<fn(RenderInfo<S>)>,

    _resize_closure: Closure::<dyn Fn()>,
    resize_observer: web_sys::ResizeObserver,
    on_resize: Rc<OnceCell<fn(&mut S, (u32, u32)) -> (u32, u32)>>,

    event_listeners: Vec<EventListener<'static>>,

    updates_per_second: u32,
    fixed_time_step: f64,
    max_frame_time: f64,
    accumulated_time: f64,
    exit: bool,
    previous_instant: f64,

    number_of_updates: u32,
    number_of_renders: u32,
}

struct EventListener<'a> {
    canvas: Rc<HtmlCanvasElement>,
    event_type: &'a str,
    closure: Closure::<dyn Fn(JsValue)>,
}
impl Drop for EventListener<'_> {
    fn drop(&mut self) {
        let _ = self.canvas.remove_event_listener_with_callback(self.event_type, self.closure.as_ref().unchecked_ref());
    }
}

pub struct UpdateInfo<'a, S: 'static> {
    pub state: &'a mut S,
    renderer: &'a mut Renderer<S>,
} impl<'a, S> UpdateInfo<'a, S> {
    pub fn exit(&mut self) {
        self.renderer.exit = true;
    }
    pub fn set_updates_per_second(&mut self, new_updates_per_second: u32) {
        self.renderer.updates_per_second = new_updates_per_second;
    }
    pub fn fixed_time_step(&self) -> f64 {
        self.renderer.fixed_time_step
    }
    pub fn number_of_updates(&self) -> u32 {
        self.renderer.number_of_updates
    }
    pub fn number_of_renders(&self) -> u32 {
        self.renderer.number_of_renders
    }
}
pub struct RenderInfo<'a, S: 'static> {
    pub state: &'a mut S,
    renderer: &'a mut Renderer<S>,
} impl<'a, S> RenderInfo<'a, S> {
    pub fn context(&'a self) -> &'a web_sys::WebGl2RenderingContext {
        &self.renderer.context
    }
    pub fn exit(&mut self) {
        self.renderer.exit = true;
    }
    pub fn set_updates_per_second(&mut self, new_updates_per_second: u32) {
        self.renderer.updates_per_second = new_updates_per_second;
    }
    pub fn fixed_time_step(&self) -> f64 {
        self.renderer.fixed_time_step
    }
    pub fn number_of_updates(&self) -> u32 {
        self.renderer.number_of_updates
    }
    pub fn number_of_renders(&self) -> u32 {
        self.renderer.number_of_renders
    }
    pub fn re_accumulate(&mut self) {
        self.renderer.accumulate(current_instant());
    }
    pub fn blending_factor(&self) -> f64 {
        self.renderer.accumulated_time / self.renderer.fixed_time_step
    }
}

impl<S> Drop for Renderer<S> {
    fn drop(&mut self) {
        self.resize_observer.disconnect();
    }
}

impl<S> Renderer<S> {
    pub fn from_canvas(canvas: HtmlCanvasElement) -> Result<Renderer<S>, JsValue> {
        
        // makes canvas focusable and thus able to recieve key* events
        canvas.set_tab_index(0); // would use 1 but docs suggest only -1 and 0 should be used

        let context = canvas
            .get_context("webgl2")?
            .unwrap()
            .dyn_into::<WebGl2RenderingContext>()?;

        let context = Rc::new(context);
        let canvas = Rc::new(canvas);
        let state = Rc::new(OnceCell::<RefCell<S>>::new());
        let on_resize = Rc::new(OnceCell::new());

        let rc_canvas = canvas.clone();
        let rc_context = context.clone();
        let rc_state = state.clone();
        let rc_on_resize = on_resize.clone();
        let resize_closure = Closure::<dyn Fn()>::new(move || {
            if let Some(state) = rc_state.get() {
                resize_canvas(&rc_canvas, &rc_context, state.borrow_mut().deref_mut(), rc_on_resize.get())
            }
        });
        let resize_observer = web_sys::ResizeObserver::new(resize_closure.as_ref().unchecked_ref())?;
        resize_observer.observe(&canvas);
        
        Ok(Renderer {
            canvas,
            context,
            state,
            
            on_update: OnceCell::new(),
            on_render: OnceCell::new(),

            _resize_closure: resize_closure,
            resize_observer,
            on_resize,

            event_listeners: Vec::new(),

            updates_per_second: 0,
            fixed_time_step: 0.0,
            max_frame_time: 0.0,
            accumulated_time: 0.0,
            exit: false,
            previous_instant: 0.0,
            number_of_updates: 0,
            number_of_renders: 0,
        })
    }

    /// consumes self and starts the game loop.
    pub fn start(mut self, state: S, updates_per_second: u32, max_frame_time: f64) {
        let _ = self.state.set(RefCell::new(state));
        self.updates_per_second = updates_per_second;
        self.fixed_time_step = 1.0 / updates_per_second as f64;
        self.max_frame_time = max_frame_time;
        self.next_frame()
        // game_loop(self, updates_per_second, max_frame_time, Self::update, Self::render);
    }

    /// links shaders to a program and attaches the program to the context to allow for drawing
    /// 
    /// returns self for chaining
    pub fn with_shaders(self, vert_shader: &str, frag_shader: &str) -> Result<Self, String> {
        let vert_shader = compile_shader(&self.context, WebGl2RenderingContext::VERTEX_SHADER, vert_shader)
            .map_err(|err| String::from("vertex shader: ") + &err)?;

        let frag_shader = compile_shader(&self.context, WebGl2RenderingContext::FRAGMENT_SHADER, frag_shader)
            .map_err(|err| String::from("fragment shader: ") + &err)?;

        let program = link_program(&self.context, &vert_shader, &frag_shader)?;
        self.context.use_program(Some(&program));
        Ok(self)
    }

    /// adds an `on_update` function that is called `updates_per_second` times per second
    /// 
    /// returns self for chaining
    /// 
    /// errors if `on_update` has already been set
    pub fn with_on_update(self, on_update: fn(UpdateInfo<S>)) -> Result<Self, ()> {
        self.on_update.set(on_update).map_err(|_| ())?;
        Ok(self)
    }
    fn update(&mut self) {
        if let Some(on_update) = self.on_update.get() {
            on_update(UpdateInfo {
                state: self.state.clone().get().unwrap().borrow_mut().deref_mut(),
                renderer: self,
            });
        }
    }

    /// adds an `on_render` function that is called as often as is allowed by the web page
    /// 
    /// returns self for chaining
    /// 
    /// errors if `on_render` has already been set
    pub fn with_on_render(self, on_render: fn(RenderInfo<S>)) -> Result<Self, ()> {
        self.on_render.set(on_render).map_err(|_| ())?;
        Ok(self)
    }
    fn render(&mut self) {
        if let Some(on_render) = self.on_render.get() {
            on_render(RenderInfo {
                state: self.state.clone().get().unwrap().borrow_mut().deref_mut(),
                renderer: self,
            });
        }
    }

    /// adds a custom event listener (that will not receive events until `start` is called) with a callback that has an `Event` argument.
    /// For most event types, this should be casted to the appropriate `*Event`:
    /// ```
    /// renderer = renderer.with_on_event("keydown", on_keydown).unwrap();
    /// ...
    /// fn on_keydown(event: web_sys::Event, state: &mut S) {
    ///     let keyboard_event = event.dyn_into::<web_sys::KeyboardEvent>().unwrap();
    ///     ...
    /// }
    /// ```
    /// 
    /// returns self for chaining
    /// 
    /// errors if on_resize has already been set
    pub fn with_on_event(mut self, event_type: &'static str, on_event: fn(&mut S, web_sys::Event)) -> Result<Self, JsValue> {
        let rc_state = self.state.clone();
        let closure = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
            if let Some(state) = rc_state.get() { // if state has been set then the loop has been started
                on_event(state.borrow_mut().deref_mut(), event.dyn_into::<Event>().unwrap())
            }
        });
        self.canvas.add_event_listener_with_callback(event_type, closure.as_ref().unchecked_ref())?;

        let event_listener = EventListener {
            canvas: self.canvas.clone(),
            event_type,
            closure,
        };
        self.event_listeners.push(event_listener);
        Ok(self)
    }

    /// adds an 'on_resize' event listener (that also optionally mutates the size)
    /// 
    /// returns self for chaining
    /// 
    /// errors if on_resize has already been set
    pub fn with_on_resize(self, on_resize: fn(&mut S, (u32, u32)) -> (u32, u32)) -> Result<Self, ()> {
        self.on_resize.set(on_resize).map_err(|_| ())?;
        Ok(self)
    }

    fn next_frame(mut self) {
        if self.exit { return }

        let current_instant = current_instant();

        self.accumulate(current_instant);

        while self.accumulated_time >= self.fixed_time_step {
            Self::update(&mut self);

            self.accumulated_time -= self.fixed_time_step;
            self.number_of_updates += 1;
        }

        // self.blending_factor = self.accumulated_time / self.fixed_time_step;

        Self::render(&mut self);
        self.number_of_renders += 1;

        self.previous_instant = current_instant;
        
        let closure = Closure::once_into_js(move || self.next_frame());
        window().unwrap().request_animation_frame(closure.as_ref().unchecked_ref()).unwrap();
    }

    fn accumulate(&mut self, current_instant: f64) {
        let mut elapsed = current_instant - &self.previous_instant;
        if elapsed > self.max_frame_time { elapsed = self.max_frame_time; }

        // self.running_time += elapsed;
        self.accumulated_time += elapsed;
    }
}

/// returns time since `timeOrigin` in seconds
///
fn current_instant() -> f64 {
    window().unwrap().performance().unwrap().now() / 1000.0
}

fn resize_canvas<S>(canvas: &HtmlCanvasElement, context: &WebGl2RenderingContext, state: &mut S, on_resize: Option<&fn(&mut S, (u32, u32)) -> (u32, u32)>) {

    let mut width = canvas.client_width() as u32;
    let mut height = canvas.client_height() as u32;
    if let Some(on_resize) = on_resize {
        (width, height) = on_resize(state, (width, height));
    }
    canvas.set_width(width);
    canvas.set_height(height);
    context.viewport(0, 0, width as i32, height as i32);
}

fn compile_shader(context: &WebGl2RenderingContext, shader_type: u32, source: &str) -> Result<WebGlShader, String> {
    let shader = context.create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("Unknown error creating shader")))
    }
}

fn link_program(context: &WebGl2RenderingContext, vert_shader: &WebGlShader, frag_shader: &WebGlShader) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create program object"))?;

    context.attach_shader(&program, vert_shader);
    context.attach_shader(&program, frag_shader);
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("Unknown error linking shader objects to program object")))
    }
}