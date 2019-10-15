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
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::GLProfile;
use std::env;

use env_logger;
use pdf::file::File as PdfFile;
use pdf::error::PdfError;
use view::Cache;

fn main() -> Result<(), PdfError> {
    env_logger::init();
    
    let path = env::args().nth(1).expect("no file given");
    println!("read: {}", path);
    let file = PdfFile::<Vec<u8>>::open(&path)?;
    
    let mut current_page = 0;
    let mut cache = Cache::new();
    // Render the canvas to screen.
    let scene: Scene = cache.render_page(&file, &*file.get_page(current_page)?)?;
    let size = scene.view_box().size();
    
    // Set up SDL2.
    let sdl_context = sdl2::init().unwrap();
    let video = sdl_context.video().unwrap();

    // Make sure we have at least a GL 3.0 context. Pathfinder requires this.
    let gl_attributes = video.gl_attr();
    gl_attributes.set_context_profile(GLProfile::Core);
    gl_attributes.set_context_version(3, 3);

    let mut scale = Vector2F::splat(2.0);
    // Open a window.
    let mut window_size = (size * scale).to_i32();
    let window = video.window("Probably Distorted File", window_size.x() as u32, window_size.y() as u32)
                      .opengl()
                      .build()
                      .unwrap();

    // Create the GL context, and make it current.
    let gl_context = window.gl_create_context().unwrap();
    gl::load_with(|name| video.gl_get_proc_address(name) as *const _);
    window.gl_make_current(&gl_context).unwrap();

    // Create a Pathfinder renderer.
    let mut renderer = Renderer::new(GLDevice::new(GLVersion::GL3, 0),
                                     &EmbeddedResourceLoader,
                                     DestFramebuffer::full_window(window_size),
                                     RendererOptions { background_color: Some(ColorF::white()) });

    let proxy = SceneProxy::from_scene(scene, RayonExecutor);
    proxy.set_view_box(
        RectF::new(Vector2F::default(), window_size.to_f32())
    );
    let mut options = BuildOptions {
        transform: RenderTransform::Transform2D(Transform2F::from_scale(scale)),
        dilation: Vector2F::default(),
        subpixel_aa_enabled: false
    };
    proxy.build_and_render(&mut renderer, options.clone());
    window.gl_swap_window();

    // Wait for a keypress.
    let mut event_pump = sdl_context.event_pump().unwrap();
    loop {
        let mut needs_update = false;
        match event_pump.wait_event() {
            Event::Quit {..} | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break,
            Event::KeyDown { keycode: Some(keycode), .. } => {
                match keycode {
                    Keycode::Left => {
                        if current_page > 0 {
                            current_page -= 1;
                            needs_update = true;
                        }
                    }
                    Keycode::Right => {
                        if current_page < file.num_pages() - 1 {
                            current_page += 1;
                            needs_update = true;
                        }
                    }
                    Keycode::Plus | Keycode::KpPlus => {
                        scale = scale * Vector2F::splat(2.0f32.sqrt());
                        needs_update = true;
                    }
                    Keycode::Minus | Keycode::KpMinus => {
                        scale = scale * Vector2F::splat(0.5f32.sqrt());
                        needs_update = true;
                    }
                    _ => {}
                }
            }
            Event::KeyUp { .. } => {}
            Event::Window { win_event: WindowEvent::Exposed, .. } => {
                proxy.build_and_render(&mut renderer, options.clone());
                window.gl_swap_window();
            }
            /*
            Event::WindowResized(new_size) => {
                window_size = new_size;
                proxy.set_view_box(
                    RectF::new(Vector2F::default(), window_size.to_f32())
                );
                renderer.set_main_framebuffer_size(window_size.device_size());
                needs_update = true;
            }*/
            e => println!("{:?}", e)
        }
        if needs_update {
            println!("showing page {}", current_page);
            let page = file.get_page(current_page)?;
            let scene = cache.render_page(&file, &page)?;
            proxy.replace_scene(scene);
            proxy.set_view_box(
                RectF::new(Vector2F::default(), window_size.to_f32())
            );
            options.transform = RenderTransform::Transform2D(Transform2F::from_scale(scale));
            proxy.build_and_render(&mut renderer, options.clone());
            window.gl_swap_window();
        }
    }
    
    Ok(())
}
