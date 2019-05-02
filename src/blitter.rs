use sw_composite::*;

use euclid::Transform2D;


pub trait Blitter {
    fn blit_span(&mut self, y: i32, x1: i32, x2: i32);
}

pub struct MaskSuperBlitter {
    width: i32,
    height: i32,
    pub buf: Vec<u8>,
}

const SHIFT: i32 = 2;
const SCALE: i32 = (1 << SHIFT);
const MASK: i32 = (SCALE - 1);
const SUPER_MASK: i32 = ((1 << SHIFT) - 1);

fn coverage_to_alpha(mut aa: i32) -> u8
{
    aa <<= 8 - 2 * SHIFT;
    aa -= aa >> (8 - SHIFT - 1);
    return aa as u8;
}

impl MaskSuperBlitter {
    pub fn new(width: i32, height: i32) -> MaskSuperBlitter {
        MaskSuperBlitter { width, height, buf: vec![0; (width * height) as usize] }
    }
}

impl Blitter for MaskSuperBlitter {
    fn blit_span(&mut self, y: i32, x1: i32, x2: i32) {
        let max: u8 = ((1 << (8 - SHIFT)) - (((y & MASK) + 1) >> SHIFT)) as u8;
        let mut b: *mut u8 = &mut self.buf[(y / 4 * self.width + (x1 >> SHIFT)) as usize];

        let mut fb = x1 & SUPER_MASK;
        let fe = x2 & SUPER_MASK;
        let mut n = (x2 >> SHIFT) - (x1 >> SHIFT) - 1;

        // invert the alpha on the left side
        if n < 0 {
            unsafe { *b += coverage_to_alpha(fe - fb) };
        } else {
            fb = (1 << SHIFT) - fb;
            unsafe { *b += coverage_to_alpha(fb) };
            unsafe { b = b.offset(1); };
            while n != 0 {
                unsafe { *b += max };
                unsafe { b = b.offset(1) };

                n -= 1;
            }
            unsafe { *b += coverage_to_alpha(fe) };
        }
    }
}

trait Shader {
    fn shade_span(&self, x: i32, y: i32, dest: &mut [u32], count: usize);
}

struct SolidShader {
    color: u32,
}

impl Shader for SolidShader {
    fn shade_span(&self, x: i32, y: i32, dest: &mut [u32], count: usize) {
        for i in 0..count {
            dest[i] = self.color;
        }
    }
}

fn transform_to_fixed(transform: &Transform2D<f32>) -> MatrixFixedPoint {
    MatrixFixedPoint {
        // Is the order right?
        xx: float_to_fixed(transform.m11),
        xy: float_to_fixed(transform.m12),
        yx: float_to_fixed(transform.m21),
        yy: float_to_fixed(transform.m22),
        x0: float_to_fixed(transform.m31),
        y0: float_to_fixed(transform.m32)
    }
}

struct ImageShader<'a> {
    image: &'a Image,
    xfm: MatrixFixedPoint,
}

impl<'a> ImageShader<'a> {
    fn new(image: &'a Image, transform: &Transform2D<f32>) -> ImageShader<'a> {
        ImageShader {
            image,
            xfm: transform_to_fixed(transform)
        }
    }
}

impl<'a> Shader for ImageShader<'a> {
    fn shade_span(&self, mut x: i32, y: i32, dest: &mut [u32], count: usize) {
        for i in 0..count {
            let p = self.xfm.transform(x as u16, y as u16);
            dest[i] = fetch_bilinear(self.image, p.x, p.y);
            x += 1;
        }
    }
}

struct GradientShader {
    gradient: Box<GradientSource>,
}

impl GradientShader {
    fn new(gradient: &Gradient, transform: &Transform2D<f32>) -> GradientShader {
        GradientShader {
            gradient: gradient.make_source(&transform_to_fixed(transform))
        }
    }
}

impl Shader for GradientShader {
    fn shade_span(&self, mut x: i32, y: i32, dest: &mut [u32], count: usize) {
        for i in 0..count {
            dest[i] = self.gradient.radial_gradient_eval(x as u16, y as u16);
            x += 1;
        }
    }
}

pub struct ShaderBlitter<'a> {
    shader: &'a Shader,
    mask: &'a [u8],
    dest: &'a mut [u32],
    tmp: Box<[u32]>,
    dest_stride: i32,
    mask_stride: i32,
}

impl<'a> Blitter for ShaderBlitter<'a> {
    fn blit_span(&mut self, y: i32, x1: i32, x2: i32) {
        let dest_row = y * self.dest_stride;
        let mask_row = y * self.mask_stride;
        let count = (x2 - x1) as usize;
        self.shader.shade_span(x1, y, &mut self.tmp[..], count);
        for i in 0..count {
            self.dest[(dest_row + x1) as usize + i] =
                over_in(self.tmp[i],
                        self.dest[(dest_row + x1) as usize + i],
                        self.mask[(mask_row + x1) as usize + i] as u32);
        }
    }
}

pub struct SolidBlitter<'a> {
    color: u32,
    mask: &'a [u8],
    dest: &'a mut [u32],
    dest_stride: i32,
    mask_stride: i32,
}

impl<'a> Blitter for SolidBlitter<'a> {
    fn blit_span(&mut self, y: i32, x1: i32, x2: i32) {
        let dest_row = y * self.dest_stride;
        let mask_row = y * self.mask_stride;
        for i in x1..x2 {
            self.dest[(dest_row + i) as usize] =
                over_in(self.color,
                        self.dest[(dest_row + i) as usize],
                        self.mask[(mask_row + i) as usize] as u32);
        }
    }
}