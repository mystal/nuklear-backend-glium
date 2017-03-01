extern crate nuklear_rust;

#[macro_use]
extern crate glium;

use nuklear_rust::{NkHandle, NkContext, NkConvertConfig, NkVec2, NkBuffer, NkDrawVertexLayoutElements, NkDrawVertexLayoutAttribute, NkDrawVertexLayoutFormat};

#[derive(Debug, Copy, Clone)]
struct Vertex {
    pos: NkVec2,
    tex: NkVec2,
    col: [u8; 4],
}

impl glium::vertex::Vertex for Vertex {
    fn build_bindings() -> glium::vertex::VertexFormat {
        use std::mem::transmute;

        unsafe {
            let dummy: &Vertex = ::std::mem::transmute(0usize);
            ::std::borrow::Cow::Owned(vec![("Position".into(), transmute(&dummy.pos), <(f32, f32) as glium::vertex::Attribute>::get_type()),
                                           ("TexCoord".into(), transmute(&dummy.tex), <(f32, f32) as glium::vertex::Attribute>::get_type()),
                                           ("Color".into(), transmute(&dummy.col), glium::vertex::AttributeType::U8U8U8U8)])
        }
    }
}

impl Default for Vertex {
    fn default() -> Self {
        unsafe { ::std::mem::zeroed() }
    }
}

const VS: &'static str = "#version 150
        uniform mat4 ProjMtx;
        in vec2 Position;
        in vec2 TexCoord;
        in vec4 Color;
        out vec2 Frag_UV;
        out vec4 Frag_Color;
        void main() {
           Frag_UV = \
                          TexCoord;
           Frag_Color = Color / 255.0;
           gl_Position = ProjMtx * vec4(Position.xy, 0, 1);
        }";
const FS: &'static str = "#version 150
        precision mediump float;
	    uniform sampler2D Texture;
        in vec2 Frag_UV;
        in vec4 Frag_Color;
        out vec4 Out_Color;
        void main(){
           Out_Color = Frag_Color * \
                          texture(Texture, Frag_UV.st);
		}";

struct TextureEntry {
    tex: glium::Texture2d,
    sampler_opts: Option<glium::uniforms::SamplerBehavior>,
}

pub struct Drawer {
    cmd: NkBuffer,
    prg: glium::Program,
    tex: Vec<TextureEntry>,
    vbf: Vec<Vertex>,
    ebf: Vec<u16>,
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    vle: NkDrawVertexLayoutElements,
}

impl Drawer {
    pub fn new(display: &mut glium::Display, texture_count: usize, vbo_size: usize, ebo_size: usize, command_buffer: NkBuffer) -> Drawer {
        // NOTE: By default, assume shaders output sRGB colors.
        let program_creation_input = glium::program::ProgramCreationInput::SourceCode {
            vertex_shader: VS,
            fragment_shader: FS,
            geometry_shader: None,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            transform_feedback_varyings: None,
            outputs_srgb: true,
            uses_point_size: false,
        };

        Drawer {
            cmd: command_buffer,
            prg: glium::Program::new(display, program_creation_input).unwrap(),
            tex: Vec::with_capacity(texture_count + 1),
            vbf: vec![Vertex::default(); vbo_size * ::std::mem::size_of::<Vertex>()],
            ebf: vec![0u16; ebo_size * ::std::mem::size_of::<u16>()],
            vbo: glium::VertexBuffer::empty_dynamic(display, vbo_size * ::std::mem::size_of::<Vertex>()).unwrap(),
            ebo: glium::IndexBuffer::empty_dynamic(display,
                                                   glium::index::PrimitiveType::TrianglesList,
                                                   ebo_size * ::std::mem::size_of::<u16>())
                .unwrap(),
            vle: NkDrawVertexLayoutElements::new(&[(NkDrawVertexLayoutAttribute::NK_VERTEX_POSITION, NkDrawVertexLayoutFormat::NK_FORMAT_FLOAT, 0),
                                                   (NkDrawVertexLayoutAttribute::NK_VERTEX_TEXCOORD, NkDrawVertexLayoutFormat::NK_FORMAT_FLOAT, 8),
                                                   (NkDrawVertexLayoutAttribute::NK_VERTEX_COLOR, NkDrawVertexLayoutFormat::NK_FORMAT_R8G8B8A8, 16),
                                                   (NkDrawVertexLayoutAttribute::NK_VERTEX_ATTRIBUTE_COUNT, NkDrawVertexLayoutFormat::NK_FORMAT_COUNT, 32)]),
        }
    }

    pub fn add_texture(&mut self, display: &mut glium::Display, image: &[u8], width: u32, height: u32,
                       sampler_opts: Option<glium::uniforms::SamplerBehavior>) -> NkHandle {
        let image = glium::texture::RawImage2d {
            data: std::borrow::Cow::Borrowed(image),
            width: width,
            height: height,
            format: glium::texture::ClientFormat::U8U8U8U8,
        };
        let tex = glium::Texture2d::new(display, image).unwrap();
        let hnd = NkHandle::from_id(self.tex.len() as i32 + 1);
        self.tex.push(TextureEntry {
            tex: tex,
            sampler_opts: sampler_opts,
        });
        hnd
    }

    pub fn draw(&mut self, ctx: &mut NkContext, cfg: &mut NkConvertConfig, frame: &mut glium::Frame, scale: NkVec2) {
        use glium::{Blend, DrawParameters, Rect};
        use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter};
        use glium::Surface;

        let (ww, hh) = frame.get_dimensions();

        let ortho = [[2.0f32 / ww as f32, 0.0f32, 0.0f32, 0.0f32], [0.0f32, -2.0f32 / hh as f32, 0.0f32, 0.0f32], [0.0f32, 0.0f32, -1.0f32, 0.0f32], [-1.0f32, 1.0f32, 0.0f32, 1.0f32]];

        cfg.set_vertex_layout(&self.vle);
        cfg.set_vertex_size(::std::mem::size_of::<Vertex>());

        {
            self.vbo.invalidate();
            self.ebo.invalidate();

            let mut rvbuf = unsafe {
                ::std::slice::from_raw_parts_mut(self.vbf.as_mut() as *mut [Vertex] as *mut u8,
                                                 self.vbf.capacity())
            };
            let mut rebuf = unsafe {
                ::std::slice::from_raw_parts_mut(self.ebf.as_mut() as *mut [u16] as *mut u8,
                                                 self.ebf.capacity())
            };
            let mut vbuf = NkBuffer::with_fixed(&mut rvbuf);
            let mut ebuf = NkBuffer::with_fixed(&mut rebuf);

            ctx.convert(&mut self.cmd, &mut vbuf, &mut ebuf, &cfg);

            self.vbo.slice_mut(0..self.vbf.capacity()).unwrap().write(&self.vbf);
            self.ebo.slice_mut(0..self.ebf.capacity()).unwrap().write(&self.ebf);
        }

        let mut idx_start = 0;
        let mut idx_end;

        for cmd in ctx.draw_command_iterator(&self.cmd) {

            if cmd.elem_count() < 1 {
                continue;
            }

            let id = cmd.texture().id().unwrap();
            let tex_entry = self.find_tex_entry(id).unwrap();
            let ptr = &tex_entry.tex;

            idx_end = idx_start + cmd.elem_count() as usize;

            let x = cmd.clip_rect().x * scale.x;
            let y = cmd.clip_rect().y * scale.y;
            let w = cmd.clip_rect().w * scale.x;
            let h = cmd.clip_rect().h * scale.y;

            let sampler = if let Some(sampler_opts) = tex_entry.sampler_opts {
                glium::uniforms::Sampler(ptr, sampler_opts)
            } else {
                ptr.sampled()
                    .magnify_filter(MagnifySamplerFilter::Linear)
                    .minify_filter(MinifySamplerFilter::Nearest)
            };

            frame.draw(&self.vbo,
                      &self.ebo.slice(idx_start..idx_end).unwrap(),
                      &self.prg,
                      &uniform! {
			              ProjMtx: ortho,
			              Texture: sampler,
			          },
                      &DrawParameters {
                          blend: Blend::alpha_blending(),
                          scissor: Some(Rect {
                              left: (if x < 0f32 { 0f32 } else { x }) as u32,
                              bottom: (if y < 0f32 { 0f32 } else { hh as f32 - y - h }) as u32,
                              width: (if x < 0f32 { w + x } else { w }) as u32,
                              height: (if y < 0f32 { h + y } else { h }) as u32,
                          }),
                          backface_culling: glium::draw_parameters::BackfaceCullingMode::CullingDisabled,

                          ..DrawParameters::default()
                      })
                .unwrap();
            idx_start = idx_end;
        }
    }

    fn find_tex_entry(&self, id: i32) -> Option<&TextureEntry> {
        if id > 0 && id as usize <= self.tex.len() {
            Some(&self.tex[(id - 1) as usize])
        } else {
            None
        }
    }
}
