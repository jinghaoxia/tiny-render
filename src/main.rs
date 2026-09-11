mod model;

use image::{Rgb, RgbImage};
use model::load_obj;

// 画线段。
// 参数从 u32 改成了 i32:整幅网格投影时,顶点可能落在图像之外甚至为负
// (翻转 y 后尤其容易),需要用带符号坐标 + 越界检查来安全跳过,
// 而不是让负数被 as u32 包成巨大的正数。
// M1 的画线函数,留在 M2 填充暂时用不到(后面加调试辅助线还会用)。
#[allow(dead_code)]
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

/// 计算像素点 p 在三角形 a,b,c 中的重心坐标。
/// 返回 (λa, λb, λc),满足 λa+λb+λc = 1 且 p = λa·a + λb·b + λc·c。
///
/// 三个 λ 是用"面积比"算的:
///   λa = (PBC 面积) / (ABC 面积)
///   λb = (PCA 面积) / (ABC 面积)
///   λc = (PAB 面积) / (ABC 面积)
///
/// 2D 叉积就是 2 倍有符号面积,所以可以省掉除 2。
/// point_in_triangle 已提供 2D 叉积(cross2),填下面三个 λ 即可。
fn barycentric(p: glam::Vec2, a: glam::Vec2, b: glam::Vec2, c: glam::Vec2) -> glam::Vec3 {
    // 2D 叉积:返回 (u-o)×(v-o) 的 z 分量,即 2 倍有符号面积。
    // (u.x-o.x)*(v.y-o.y) - (u.y-o.y)*(v.x-o.x)
    fn cross2(o: glam::Vec2, u: glam::Vec2, v: glam::Vec2) -> f32 {
        (u - o).perp_dot(v - o)
    }

    // TODO: 按上面的面积比公式,算出三个重心坐标。
    //   denom = cross2(a, b, c)   —— 整个三角形的 2 倍面积
    //   λa    = cross2(p, b, c) / denom
    //   λb    = cross2(p, c, a) / denom
    //   λc    = cross2(p, a, b) / denom
    // 返回 Vec3::new(λa, λb, λc)。
    // 提示:denom 和三次 cross2 的符号是否一致,决定了 λ 是否干净地对齐。
    let denom = cross2(a, b, c);
    let λa = cross2(p, b, c) / denom;
    let λb = cross2(p, c, a) / denom;
    let λc = cross2(p, a, b) / denom;
    glam::Vec3::new(λa, λb, λc)
}

/// 光栅化一个三角形:把落在它内部的像素涂成 color。
/// 参数是三个屏幕坐标(project 的输出),允许有负值/越界,内部会挡。
fn rasterize_triangle(
    img: &mut RgbImage,
    ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32,
    color: Rgb<u8>,
) {
    // 三个顶点转成 2D 浮点坐标,供重心坐标用
    let a = glam::Vec2::new(ax as f32, ay as f32);
    let b = glam::Vec2::new(bx as f32, by as f32);
    let c = glam::Vec2::new(cx as f32, cy as f32);

    // 包围盒:三角形在 x/y 上跨过的最小矩形
    let min_x = ax.min(bx).min(cx);
    let max_x = ax.max(bx).max(cx);
    let min_y = ay.min(by).min(cy);
    let max_y = ay.max(by).max(cy);

    // 夹到图像范围内,避免遍历到外面
    let min_x = min_x.max(0);
    let max_x = max_x.min(img.width() as i32 - 1);
    let min_y = min_y.max(0);
    let max_y = max_y.min(img.height() as i32 - 1);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            // 用像素中心而不是像素角 + 0.5 偏移,边界才对称
            let p = glam::Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let bc = barycentric(p, a, b, c);
            // 三个重心坐标都 >= 0 ⟺ 在三角形内(含边界)
            if bc.min_element() >= 0.0 {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 800u32;
    let height = 800u32;
    let mut img = RgbImage::new(width, height);

    let model = load_obj("resource/klee.obj")?;
    eprintln!(
        "v={} vt={} vn={} f={}",
        model.positions.len(),
        model.uvs.len(),
        model.normals.len(),
        model.faces.len()
    );

    // 遍历每个三角形并光栅化填充。
    // 暂时给每个三角形不同的颜色:能一眼看出"哪个三角形光栅化错了"。
    // 全部正确后,把颜色统一成白色即可。
    for (i, face) in model.faces.iter().enumerate() {
        // 三个顶点坐标(模型空间)
        let pa = model.positions[face.v[0]];
        let pb = model.positions[face.v[1]];
        let pc = model.positions[face.v[2]];

        // 投影到屏幕
        let (ax, ay) = project(pa, width, height);
        let (bx, by) = project(pb, width, height);
        let (cx, cy) = project(pc, width, height);

        // 用一个简单的面下标公式区分颜色
        let r = ((i * 37) % 255) as u8;
        let g = ((i * 163) % 255) as u8;
        let bl = ((i * 89) % 255) as u8;
        let color = Rgb([r, g, bl]);

        rasterize_triangle(&mut img, ax, ay, bx, by, cx, cy, color);
    }

    img.save("output.png")?;
    Ok(())
}
