use glam::Vec3;

/// 一个三角面:每个角带三个下标,分别指向 positions / uvs / normals。
/// 注意 obj 文件里下标是 1-based,本结构存的是转成 0-based 之后的值。
#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub v: [usize; 3],
    pub vt: [usize; 3],
    pub vn: [usize; 3],
}

#[derive(Debug, Default)]
pub struct Model {
    pub positions: Vec<Vec3>, // v  : 顶点坐标
    pub uvs: Vec<Vec3>,       // vt : 纹理坐标(只用到 x,y)
    pub normals: Vec<Vec3>,   // vn : 顶点法线
    pub faces: Vec<Face>,     // f  : 三角形(每个角引用上面三个数组)
}

/// 纯内存解析,不碰磁盘 —— 测试直接喂字符串,不必造临时文件。
/// 这也是读文件版 load_obj 的内部实现。
pub fn load_obj_str(src: &str) -> Model {
    let mut m = Model::default();

    for line in src.lines() {
        let line = line.trim();
        // 空行、注释行(任意空白后跟 #)直接跳过
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 按空白切词:第一个词是类型关键字,后面是数据
        let mut parts = line.split_whitespace();
        let kind = parts.next().unwrap(); // 前面已保证非空,这里不会 panic

        match kind {
            "v" => {
                // 示例:把剩余词逐一解析成 f32,再组成 Vec3。
                // parts 是迭代器,每调一次 next() 吐一个词。
                let x: f32 = parts.next().expect("顶点缺 x").parse().expect("顶点 x 不是数字");
                let y: f32 = parts.next().expect("顶点缺 y").parse().expect("顶点 y 不是数字");
                let z: f32 = parts.next().expect("顶点缺 z").parse().expect("顶点 z 不是数字");
                m.positions.push(Vec3::new(x, y, z));
            }
            "vt" => {
                // 照上面 v 的样子写,但纹理只有两个分量 u、v,
                // z 分量用 0.0 占位,方便统一存 Vec3。
                let u: f32 = parts.next().expect("纹理缺 u").parse().expect("纹理 u 不是数字");
                let v: f32 = parts.next().expect("纹理缺 v").parse().expect("纹理 v 不是数字");
                m.uvs.push(Vec3::new(u, v, 0.0));
            }
            "vn" => {
                // 法线也是三个分量,照抄 v 分支即可。
                let x: f32 = parts.next().expect("法线缺 x").parse().expect("法线 x 不是数字");
                let y: f32 = parts.next().expect("法线缺 y").parse().expect("法线 y 不是数字");
                let z: f32 = parts.next().expect("法线缺 z").parse().expect("法线 z 不是数字");
                m.normals.push(Vec3::new(x, y, z));
            }
            "f" => {
                // 每行是三个词,形如 "4/4/4" "3/3/3" "2/2/2"。
                // 对每个词:
                //   1. 按 '/' 切成三段,依次是 v、vt、vn 的下标字符串;
                //   2. 各自 parse 成整数(先当 i32,见下一条);
                //   3. obj 是 1-based → 存进数组前减 1 变 0-based。
                // 注意:真正的 obj 规范允许负数(相对当前面的索引),
                // 所以建议先用 i32 存,减完 1 确认非负后再 as usize。
                // 本例的小技巧:本文件三个下标编号总是一样,但你写的时候
                // 别依赖它,把三段都解析出来存好(后面 M4/M5 要用 vt、vn)。
                let mut f = Face {
                    v: [0;3],
                    vt: [0;3],
                    vn: [0;3]
                };
                for i in 0..3 {
                    // 取一个角,例如 "4/4/4"
                    let corner = parts.next().expect("面缺角");
                    // 把 "4/4/4" 按 '/' 拆开 → 依次是 v、vt、vn 的文本下标
                    let mut idx = corner.split('/');
                    let x: i32 = idx.next().expect("缺少面顶点").parse().expect("面顶点不是数字");
                    let y: i32 = idx.next().expect("缺少面顶点").parse().expect("面顶点不是数字");
                    let z: i32 = idx.next().expect("缺少面顶点").parse().expect("面顶点不是数字");
                    f.v[i] = (x - 1) as usize;
                    f.vt[i] = (y - 1) as usize;
                    f.vn[i] = (z - 1) as usize;
                }
                m.faces.push(f);
            }
            // mtllib / usemtl / o / g / s / 3D 工具自定义头:一律忽略
            _ => {}
        }
    }

    m
}

/// 读文件版:打开文件、读出全部文本,再交给纯内存解析。
pub fn load_obj(path: &str) -> Result<Model, std::io::Error> {
    let src = std::fs::read_to_string(path)?;
    Ok(load_obj_str(&src))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_ok() {
        let m = load_obj_str("");
        assert!(m.positions.is_empty());
        assert!(m.faces.is_empty());
    }

    #[test]
    fn comment_and_blank_lines_are_skipped() {
        // 注：空行、# 注释、mtllib 都应被跳过，只有一行 v 被解析
        let src = "\
# By https://any3d.cc
mtllib klee.mtl

v 0.0 0.0 0.0
";
        let m = load_obj_str(src);
        assert_eq!(m.positions.len(), 1);
        assert_eq!(m.positions[0], Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn full_triangle_roundtrip() {
        // 一个带 v/vt/vn 的三角形,下标 1-based。
        // 这个测试要等你填完 f 分支才会通过。
        let src = "\
v 1.0 2.0 3.0
vt 0.5 0.25
vn 0.0 0.0 1.0
f 1/1/1 1/1/1 1/1/1
";
        let m = load_obj_str(src);
        assert_eq!(m.positions.len(), 1);
        assert_eq!(m.uvs.len(), 1);
        assert_eq!(m.normals.len(), 1);
        assert_eq!(m.faces.len(), 1);
        // 关键断言:1-based 的下标 1 应转成 0-based 的 0
        assert_eq!(m.faces[0].v, [0, 0, 0]);
        assert_eq!(m.faces[0].vt, [0, 0, 0]);
        assert_eq!(m.faces[0].vn, [0, 0, 0]);
    }
}
