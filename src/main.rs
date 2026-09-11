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

/// look-at 相机:求"世界 → 相机空间"的视图矩阵。
/// eye = 相机摆在哪,center = 看向哪,up = 世界里的"天上方向"。
/// 得到的矩阵把相机挪到原点、视线对准 +z 轴:之后相机空间里的 z
/// 就有了物理意义 —— 点离相机沿视线方向的距离,z 越大越远。
fn lookat(eye: glam::Vec3, center: glam::Vec3, up: glam::Vec3) -> glam::Mat4 {
    // TODO: 相机的三个基向量(坐标轴)。几何直觉:
    //   f(forward):从 eye 指向 center 的单位向量 —— 相机"看哪"
    //   s(right)  :f × up 再单位化 —— 相机的"右手边"(垂直于视线和天)
    //   u(up)     :s × f —— 修正后的"头顶方向"(f⊥s 且都单位,u 自然是单位)
    //   直觉:拿"看哪/右手边/头顶"三根互相垂直的轴,就能描述相机的全部姿态;
    //   叉积正是在造"垂直于已知两根轴"的新轴。
    let f: glam::Vec3 = (center - eye).normalize();
    let s: glam::Vec3 = f.cross(up).normalize();
    let u: glam::Vec3 = s.cross(f);

    // 装配视图矩阵(机械步骤,我写好):s/u/f 分别当矩阵的三行,
    // 平移列取"负点积"把 eye 平移到原点。glam 是列主序,所以行要竖着塞进列。
    glam::Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, f.x, 0.0),
        glam::Vec4::new(s.y, u.y, f.y, 0.0),
        glam::Vec4::new(s.z, u.z, f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), -f.dot(eye), 1.0),
    )
}

/// 相机空间点 → 屏幕像素。
/// ca 是 lookat 之后的坐标:相机在原点、视线沿 +z,可见点 z > 0,z 越大越远。
///
/// 你 M1 写的三步(scale/居中/翻 y)全部保留,只在最前面多一步:
///   0. 透视除法:把 x、y 除以 z —— 同样的横向偏移,点越远投影越靠中间,
///      这就是"近大远小"的全部数学。除完数量级变小,后面 scale 负责拉回屏幕尺寸。
fn project(ca: glam::Vec3, w: u32, h: u32) -> (i32, i32) {
    let scale: f32 = 800.0; // 旋钮:画面嫌小加大,裁到边减小(现在除过 z,和 M1 的 700 不是一个量纲)
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;

    // TODO: 透视除法,就两行
    let px: f32 = ca.x / ca.z;
    let py: f32 = ca.y / ca.z;

    let sx = cx + px * scale;
    let sy = cy - py * scale;
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

    //   按上面的面积比公式,算出三个重心坐标。
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

/// 光栅化一个三角形,逐像素调用 shade(x, y, bc) 决定颜色。
///
/// M4 的分工:光栅化只负责两件事——
///   1. 哪些像素属于这个三角形(包围盒 + 重心坐标内外判断);
///   2. 哪个三角形占据该像素(深度测试)。
/// 至于该像素该是什么颜色,交给 shade(x, y, bc) 去算:
/// 它拿到像素坐标和该像素的插值重心坐标 bc,可用来插值法线/uv 等。
/// shade 返回 Option<Rgb<u8>>(None 表示该像素无需画)。
fn rasterize_triangle(
    img: &mut RgbImage,
    zbuf: &mut [f32],
    ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32,
    za: f32, zb: f32, zc: f32,
    shade: impl Fn(i32, i32, glam::Vec3) -> Option<Rgb<u8>>,
) {
    let a = glam::Vec2::new(ax as f32, ay as f32);
    let b = glam::Vec2::new(bx as f32, by as f32);
    let c = glam::Vec2::new(cx as f32, cy as f32);
    let w = img.width() as usize;

    let min_x = ax.min(bx).min(cx);
    let max_x = ax.max(bx).max(cx);
    let min_y = ay.min(by).min(cy);
    let max_y = ay.max(by).max(cy);

    let min_x = min_x.max(0);
    let max_x = max_x.min(img.width() as i32 - 1);
    let min_y = min_y.max(0);
    let max_y = max_y.min(img.height() as i32 - 1);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = glam::Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let bc = barycentric(p, a, b, c);
            if bc.min_element() >= 0.0 {
                let z = bc.x * za + bc.y * zb + bc.z * zc;
                let idx = y as usize * w + x as usize;
                // 相机空间 z 越小越近,所以"更近"用 < 判定
                if z < zbuf[idx] {
                    // 此像素由这个三角形占据;问着色函数它的颜色
                    if let Some(color) = shade(x, y, bc) {
                        img.put_pixel(x as u32, y as u32, color);
                    }
                    zbuf[idx] = z;
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 800u32;
    let height = 800u32;
    let mut img = RgbImage::new(width, height);

    // 深度缓冲:每像素记录"当前看到的最近深度"。
    // M4 深度取自相机空间的 z(视线距离),z 越小越近、越大越远;
    // 初始为"无穷远",任何真实距离都比它小 → 会被第一个三角形替换。
    let mut zbuf = vec![f32::INFINITY; (width * height) as usize];

    // —— 相机与光照参数(想换视角/换灯光只改这三行)——
    let eye = glam::Vec3::new(0.0, 0.1, 1.5);   // 相机摆在哪(正面稍高处)
    let look_at = glam::Vec3::ZERO;             // 看向模型中心
    let light_dir = glam::Vec3::new(0.3, 0.5, -1.0).normalize(); // 光的传播方向(从光向外射)

    // Blinn-Phong 材质系数(想调光感就改这几行)
    const AMBIENT: f32 = 0.10;   // 环境项:兜底背光面,避免纯黑
    const DIFFUSE_K: f32 = 0.70; // 漫反射权重
    const SPEC_K: f32 = 0.45;    // 高光权重
    const SHININESS: f32 = 32.0; // 高光锐度:越大 → 光斑越小越亮

    let view = lookat(eye, look_at, glam::Vec3::Y);
    let u8_from = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;

    let model = load_obj("resource/klee.obj")?;
    eprintln!(
        "v={} vt={} vn={} f={}",
        model.positions.len(),
        model.uvs.len(),
        model.normals.len(),
        model.faces.len()
    );

    for face in &model.faces {
        // 三组各自独立的顶点下标(position 用 v[·],法线用 vn[·])
        let pa = model.positions[face.v[0]];
        let pb = model.positions[face.v[1]];
        let pc = model.positions[face.v[2]];

        let na = model.normals[face.vn[0]];
        let nb = model.normals[face.vn[1]];
        let nc = model.normals[face.vn[2]];

        // 世界 → 相机空间(透视除法在 project 里做)
        let ca = view.transform_point3(pa);
        let cb = view.transform_point3(pb);
        let cc = view.transform_point3(pc);

        // 投影到屏幕
        let (ax, ay) = project(ca, width, height);
        let (bx, by) = project(cb, width, height);
        let (cx, cy) = project(cc, width, height);

        // 相机空间的 z 当深度:z 越大越远,负值=在相机身后
        rasterize_triangle(
            &mut img, &mut zbuf,
            ax, ay, bx, by, cx, cy,
            ca.z, cb.z, cc.z,
            |x, y, bc| {
                // —— Blinn-Phong 着色:插值法线 → 三项光照相加 ——
                // 法线插值:又是重心坐标加权平均,和深度一模一样,这次填好了。
                let n: glam::Vec3 = bc.x * na + bc.y * nb + bc.z * nc;
                let n = n.normalize(); // 插值会把长度揉歪,用前先归一化

                // 相机空间里相机就在原点,所以"像素 → 相机"的视线方向:
                //   V = normalize(原点 − 像素位置)
                let p_cam = bc.x * ca + bc.y * cb + bc.z * cc;
                let v: glam::Vec3 = (-p_cam).normalize();

                // 指向光源的方向(光传播方向的取反)
                let l: glam::Vec3 = -light_dir;

                // TODO: 漫反射项 —— 和 Lambert 一样的点积:
                //   diff = max(0, n·L),L 就是上面的 l
                let diff: f32 = todo!("diff = n.dot(l).max(0.0)");

                // TODO: 半程向量 H = normalize(L + V) —— Blinn-Phong 的核心。
                //   直觉:H 是"光"与"眼睛"两个方向的角平分线方向;
                //   表面法线 n 越贴合 H,说明"光正好经它反射进眼睛",高光越亮。
                let h: glam::Vec3 = todo!("h = (l + v).normalize()");

                // TODO: 高光项 spec = max(0, n·H)^shininess
                //   先夹到非负,再取 SHININESS 次幂(幂越高,光斑越小越锐)。
                let spec: f32 = todo!("spec = n.dot(h).max(0.0).powf(SHININESS)");

                // 三项相加:环境(兜底)+ 漫反射 + 高光
                let intensity = AMBIENT + DIFFUSE_K * diff + SPEC_K * spec;

                Some(Rgb([u8_from(intensity), u8_from(intensity), u8_from(intensity)]))
            },
        );
    }

    img.save("output.png")?;
    Ok(())
}
