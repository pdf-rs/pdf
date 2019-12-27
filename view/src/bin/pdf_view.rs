// pathfinder/examples/canvas_text/src/main.rs
//
// Copyright © 2019 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use pathfinder_geometry::vector::{Vector2F};
use pathfinder_geometry::rect::{RectF};
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_content::color::ColorF;
use pathfinder_gl::{GLDevice, GLVersion};
use pathfinder_gpu::resources::{EmbeddedResourceLoader};
use pathfinder_renderer::scene::Scene;
use pathfinder_renderer::concurrent::rayon::RayonExecutor;
use pathfinder_renderer::concurrent::scene_proxy::SceneProxy;
use pathfinder_renderer::gpu::options::{DestFramebuffer, RendererOptions};
use pathfinder_renderer::gpu::renderer::Renderer;
use pathfinder_renderer::options::{BuildOptions, RenderTransform};
use std::env;
use glutin::{
    event::{Event, WindowEvent, DeviceEvent, KeyboardInput, ElementState, VirtualKeyCode, MouseButton},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    dpi::{LogicalSize, LogicalPosition},
    GlRequest, Api
};
use gl;

use env_logger;
use pdf::file::File as PdfFile;
use pdf::error::PdfError;
use pdf::object::Rect;
use view::Cache;

fn main() -> Result<(), PdfError> {
    env_logger::init();
    
    let path = env::args().nth(1).expect("no file given");
    println!("read: {}", path);
    let file = PdfFile::<Vec<u8>>::open(&path)?;
    
    let mut current_page = 0;
    let mut cache = Cache::new();
    
    let page = file.get_page(current_page)?;
    let Rect { left, right, top, bottom } = page.media_box(&file).expect("no media box");
    let size = Vector2F::new(right - left, top - bottom);
    
    let event_loop = EventLoop::new();

    let mut scale = Vector2F::splat(1.0);
    let mut translation = Vector2F::new(0.0, 0.0);

    let window_size = (size * scale).to_i32();
    let window_builder = WindowBuilder::new()
        .with_title("Probably Distorted File")
        .with_inner_size(LogicalSize::new(window_size.x() as f64, window_size.y() as f64));

    let windowed_context = glutin::ContextBuilder::new()
        .with_gl(GlRequest::Specific(Api::OpenGl, (3, 0)))
        .build_windowed(window_builder, &event_loop)
        .unwrap();
    
    let windowed_context = unsafe {
        windowed_context.make_current().unwrap()
    };

    gl::load_with(|ptr| windowed_context.get_proc_address(ptr));
    
    let window = windowed_context.window();
    let dpi = Vector2F::splat(window.hidpi_factor() as f32);

    let proxy = SceneProxy::new(RayonExecutor);

    // Create a Pathfinder renderer.
    let mut renderer = Renderer::new(GLDevice::new(GLVersion::GL3, 0),
        &EmbeddedResourceLoader,
        DestFramebuffer::full_window((size * scale * dpi).to_i32()),
        RendererOptions { background_color: Some(ColorF::new(0.9, 0.85, 0.7, 1.0)) }
    );

    let mut needs_update = true;
    let mut needs_redraw = true;
    let mut drag_offset: Option<Vector2F> = None;
    let mut cursor_pos = Vector2F::default();
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::EventsCleared => {
                if needs_update {
                    println!("showing page {}", current_page);
                    let scene = match file.get_page(current_page).and_then(|page| cache.render_page(&file, &page)) {
                        Ok(scene) => scene,
                        _ => {
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    };
                    proxy.replace_scene(scene);

                    needs_update = false;
                    needs_redraw = true;
                }
                if needs_redraw {
                    let window = windowed_context.window();
                    let dpi = Vector2F::splat(window.hidpi_factor() as f32);
                    let view_size = size * scale;
                    let window_size = LogicalSize::new(view_size.x() as f64, view_size.y() as f64);
                    window.set_inner_size(window_size);
                    renderer.set_main_framebuffer_size((view_size * dpi).to_i32());
                    proxy.set_view_box(
                        RectF::new(Vector2F::default(), view_size * dpi)
                    );

                    let options = BuildOptions {
                        transform: RenderTransform::Transform2D(
                            Transform2F::from_scale(dpi) * Transform2F::from_translation(translation) * Transform2F::from_scale(scale)
                        ),
                        dilation: Vector2F::default(),
                        subpixel_aa_enabled: false
                    };
                    proxy.build_and_render(&mut renderer, options);
                    windowed_context.swap_buffers().unwrap();

                    needs_redraw = false;
                }
            },
            Event::DeviceEvent { 
                event: DeviceEvent::Key(KeyboardInput {
                    state: ElementState::Pressed,
                    virtual_keycode: Some(keycode),
                    ..
                }),
                ..
            } => {
                match keycode {
                    VirtualKeyCode::Escape => {
                        *control_flow = ControlFlow::Exit;
                    }
                    VirtualKeyCode::Left => {
                        if current_page > 0 {
                            current_page -= 1;
                            needs_update = true;
                        }
                    }
                    VirtualKeyCode::Right => {
                        if current_page < file.num_pages() - 1 {
                            current_page += 1;
                            needs_update = true;
                        }
                    }
                    VirtualKeyCode::Add => {
                        scale = scale * Vector2F::splat(2.0f32.sqrt());
                        needs_update = true;
                    }
                    VirtualKeyCode::Subtract => {
                        scale = scale * Vector2F::splat(0.5f32.sqrt());
                        needs_update = true;
                    }
                    _ => {}
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved {
                    position: LogicalPosition { x, y }, ..
                }, ..
            } => {
                cursor_pos = Vector2F::new(x as f32, y as f32);
                if let Some(offset) = drag_offset {
                    translation = cursor_pos + offset;
                    needs_redraw = true;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state, ..
                }, ..
            } => {
                drag_offset = match state {
                    ElementState::Pressed => Some(translation - cursor_pos),
                    ElementState::Released => {
                        dbg!(translation);
                        None
                    }
                };
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                needs_redraw = true;
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("The close button was pressed; stopping");
                *control_flow = ControlFlow::Exit
            },
            _ => *control_flow = ControlFlow::Wait,
        }
    });

    Ok(())
}
