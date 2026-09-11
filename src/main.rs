mod model;

use image::{Rgb, RgbImage};
use model::load_obj;

// 画线段。
// 参数从 u32 改成了 i32:整幅网格投影时,顶点可能落在图像之外甚至为负
// (翻转 y 后尤其容易),需要用带符号坐标 + 越界检查来安全跳过,
// 而不是让负数被 as u32 包成巨大的正数。
pub fn draw_line(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let mut x = x0;
    let mut y = y0;

    loop {
        if x >= 0 && x < img.width() as i32 && y >= 0 && y < img.height() as i32 {
            img.put_pixel(x as u32, y as u32, color);
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = err * 2;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// 模型坐标 → 屏幕像素坐标。
/// 模型坐标范围约 -0.5..0.5(中心在原点);图像左上角是 (0,0)、y 轴朝下。
/// 要做三件事,想清楚再写:
///   1. 放大:乘一个 scale,让模型在 800×800 里撑得够大又不裁掉边缘
///      (比如让 -0.5..0.5 这段映射到约 700px,scale≈700);
///   2. 平移到中心:算出能把它挪到图像正中间的偏移量 (cx, cy);
///   3. 翻转 y:图像 y 朝下、模型 y 朝上 → screen_y 不能用原 y,
///      要拿图像高减去"朝下读"的 y。
/// 返回值类型是 (i32,i32),允许暂时是负数/越界,画线时会被挡掉。
fn project(v: glam::Vec3, w: u32, h: u32) -> (i32, i32) {
    let scale: f32 = 700.0;                       // 跨度约 1.0 → 放大到约 700px
    let cx = w as f32 / 2.0;                      // 图像中心 x(像素坐标,直接可用)
    let cy = h as f32 / 2.0;                      // 图像中心 y

    let sx = cx + v.x * scale;                    // x:模型左右本来就对称,中心平移即可
    let sy = cy - v.y * scale;                    // y:关键一步,用"中心 减 已放大y"
    (sx as i32, sy as i32)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 800u32;
    let height = 800u32;
    let mut img = RgbImage::new(width, height);
    let white = Rgb([255, 255, 255]);

    let model = load_obj("resource/klee.obj")?;
    eprintln!(
        "v={} vt={} vn={} f={}",
        model.positions.len(),
        model.uvs.len(),
        model.normals.len(),
        model.faces.len()
    );

    // 遍历每个三角形,画它的三条边。这就是"整网格线框"。
    for face in &model.faces {
        // 三个顶点坐标
        let pa = model.positions[face.v[0]];
        let pb = model.positions[face.v[1]];
        let pc = model.positions[face.v[2]];

        // 三条边:ab、bc、ca
        let (ax, ay) = project(pa, width, height);
        let (bx, by) = project(pb, width, height);
        let (cx, cy) = project(pc, width, height);
        draw_line(&mut img, ax, ay, bx, by, white);
        draw_line(&mut img, bx, by, cx, cy, white);
        draw_line(&mut img, cx, cy, ax, ay, white);
    }

    img.save("output.png")?;
    Ok(())
}
