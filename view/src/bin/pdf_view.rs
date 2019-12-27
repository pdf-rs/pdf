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
    event::{Event, WindowEvent, DeviceEvent, KeyboardInput, ElementState, VirtualKeyCode, MouseButton, MouseScrollDelta, ModifiersState },
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    dpi::{LogicalSize, LogicalPosition, PhysicalSize},
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

    // ratio of PDF screen space to logical screen space
    let mut scale = 1.0;

    // center of the PDF screen space that is at the center of the window
    let mut view_center = Vector2F::new(right + left, top + bottom).scale(0.5);

    let mut window_size = size * Vector2F::splat(scale);
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
    let mut dpi = window.hidpi_factor() as f32;

    let proxy = SceneProxy::new(RayonExecutor);
    let mut framebuffer_size = (size * Vector2F::splat(scale * dpi)).to_i32();
    // Create a Pathfinder renderer.
    let mut renderer = Renderer::new(GLDevice::new(GLVersion::GL3, 0),
        &EmbeddedResourceLoader,
        DestFramebuffer::full_window(framebuffer_size),
        RendererOptions { background_color: Some(ColorF::new(0.9, 0.85, 0.8, 1.0)) }
    );

    let mut needs_update = true;
    let mut needs_redraw = true;
    let mut cursor_pos = Vector2F::default();
    let mut dragging = false;
    event_loop.run(move |event, _, control_flow| {
        dbg!(&event);
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
                    let physical_size = window_size * Vector2F::splat(dpi);
                    let new_framebuffer_size = physical_size.to_i32();
                    if new_framebuffer_size != framebuffer_size {
                        framebuffer_size = new_framebuffer_size;
                        windowed_context.resize(PhysicalSize::new(framebuffer_size.x() as f64, framebuffer_size.y() as f64));
                        renderer.replace_dest_framebuffer(DestFramebuffer::full_window(framebuffer_size));
                    }
                    proxy.set_view_box(RectF::new(Vector2F::default(), physical_size));
                    let options = BuildOptions {
                        transform: RenderTransform::Transform2D(
                            Transform2F::from_translation(physical_size.scale(0.5)) *
                            Transform2F::from_scale(Vector2F::splat(dpi * scale)) *
                            Transform2F::from_translation(-view_center)
                        ),
                        dilation: Vector2F::default(),
                        subpixel_aa_enabled: false
                    };
                    proxy.build_and_render(&mut renderer, options);
                    windowed_context.swap_buffers().unwrap();

                    needs_redraw = false;
                }
            },
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::HiDpiFactorChanged(new_dpi) => {
                    dpi = new_dpi as f32;
                    needs_redraw = true;
                }
                WindowEvent::Resized(LogicalSize {width, height}) => {
                    window_size = Vector2F::new(width as f32, height as f32);
                    needs_redraw = true;
                }
                WindowEvent::KeyboardInput{
                    input: KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                }  => match keycode {
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
                        scale *= 2.0f32.sqrt();
                        needs_update = true;
                    }
                    VirtualKeyCode::Subtract => {
                        scale *= 0.5f32.sqrt();
                        needs_update = true;
                    }
                    _ => {}
                },
                WindowEvent::CursorMoved { position: LogicalPosition { x, y }, .. } => {
                    let new_pos = Vector2F::new(x as f32, y as f32);
                    let cursor_delta = new_pos - cursor_pos;
                    cursor_pos = new_pos;

                    if dragging {
                        view_center = view_center - cursor_delta.scale(1.0 / scale);
                        needs_redraw = true;
                    }
                },
                WindowEvent::MouseInput { button: MouseButton::Left, state, .. } => {
                    dragging = match state {
                        ElementState::Pressed => true,
                        ElementState::Released => false
                    };
                },
                WindowEvent::MouseWheel { delta, modifiers, .. } => {
                    let delta = match delta {
                        MouseScrollDelta::PixelDelta(LogicalPosition { x: dx, y: dy }) => Vector2F::new(dx as f32, dy as f32),
                        MouseScrollDelta::LineDelta(dx, dy) => Vector2F::new(dx as f32, -dy as f32).scale(10.)
                    };
                    match modifiers {
                        ModifiersState { ctrl: false, .. } => {
                            view_center = view_center - delta.scale(1.0 / scale);
                            needs_redraw = true;
                        },
                        _ => {}
                    }
                }
                WindowEvent::RedrawRequested => {
                    needs_redraw = true;
                },
                WindowEvent::CloseRequested => {
                    println!("The close button was pressed; stopping");
                    *control_flow = ControlFlow::Exit
                },
                _ => {}
            }
            _ => *control_flow = ControlFlow::Wait,
        }
    });

    Ok(())
}
