use crate::utils::{read_raw, three_to_one};
use pdbtbx::Residue;
use std::io::BufRead;
use wide::f64x4;

fn d2_batch_wide(x: &[f64], y: &[f64], z: &[f64], p: [f64; 3]) -> Vec<f64> {
    assert!(x.len() == y.len() && y.len() == z.len());
    let n = x.len();
    let mut out = vec![0.0; n];

    let px = f64x4::splat(p[0]);
    let py = f64x4::splat(p[1]);
    let pz = f64x4::splat(p[2]);

    let chunks = n / 4 * 4;
    for j in (0..chunks).step_by(4) {
        let xj = f64x4::from([x[j], x[j + 1], x[j + 2], x[j + 3]]);
        let yj = f64x4::from([y[j], y[j + 1], y[j + 2], y[j + 3]]);
        let zj = f64x4::from([z[j], z[j + 1], z[j + 2], z[j + 3]]);

        let dx = px - xj;
        let dy = py - yj;
        let dz = pz - zj;
        let d2 = dx * dx + dy * dy + dz * dz;

        let arr = d2.as_array();
        out[j..j + 4].copy_from_slice(arr);
    }
    for j in chunks..n {
        let dx = p[0] - x[j];
        let dy = p[1] - y[j];
        let dz = p[2] - z[j];
        out[j] = dx * dx + dy * dy + dz * dz;
    }
    out
}

fn get_residue_pos(residue: &Residue) -> [f64; 3] {
    for atom in residue.atoms() {
        if atom.name() == "CA" {
            return atom.pos().into();
        }
    }

    let n = residue.atom_count();
    residue
        .atoms()
        .map(|atom| atom.pos())
        .fold([0.0, 0.0, 0.0], |[acc_x, acc_y, acc_z], (x, y, z)| {
            [acc_x + x, acc_y + y, acc_z + z]
        })
        .map(|e| e / n as f64)
}

pub fn d2_map<R: BufRead>(
    reader: R,
    chain_id: Option<String>,
    format: &str,
) -> Result<(Vec<String>, Vec<Vec<f64>>), String> {
    let (pdb, _errors) = read_raw(reader, format)?;

    let mut axis = Vec::new();
    let mut values = vec![];

    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut z = Vec::new();

    if let Some(cid) = chain_id.as_deref() {
        let chain_ids: Vec<_> = pdb.chains().map(|chain| chain.id()).collect();
        if !chain_ids.contains(&cid) {
            return Err(format!(
                "Chain {cid} not exists. Valid chain IDs are: {:?}",
                chain_ids
            ));
        }
    }

    for chain in pdb.chains() {
        if let Some(cid) = &chain_id {
            if cid != chain.id() {
                continue;
            }
        }
        for residue in chain.residues() {
            axis.push(format!(
                "{}-{}-{}",
                chain.id(),
                residue.id().0,
                three_to_one(residue.name().unwrap_or("UNK"))
            ));
            let pos = get_residue_pos(residue);
            x.push(pos[0]);
            y.push(pos[1]);
            z.push(pos[2]);
        }
    }

    let n = axis.len();

    for i in 0..n {
        let d2 = d2_batch_wide(&x[i + 1..n], &y[i + 1..n], &z[i + 1..n], [x[i], y[i], z[i]]);
        values.push(d2);
    }

    Ok((axis, values))
}

pub fn knn_map<R: BufRead>(
    reader: R,
    chain_id: Option<String>,
    k: usize,
    format: &str,
) -> Result<(Vec<String>, Vec<Vec<f64>>), String> {
    let mut d2_map = d2_map(reader, chain_id, format)?;
    let (_, values) = &mut d2_map;

    for line in values {
        let mut tmp = line.clone();
        let (_, kth, _) = tmp.select_nth_unstable_by(k, |a, b| a.total_cmp(b));

        let mut has_equal_v = false;
        for e in line {
            *e = if e < kth {
                e.sqrt()
            } else if e == kth && !has_equal_v {
                has_equal_v = true;
                e.sqrt()
            } else {
                0.0
            }
        }
    }
    let k_map = d2_map;

    Ok(k_map)
}

pub fn d_map<R: BufRead>(
    reader: R,
    chain_id: Option<String>,
    format: &str,
) -> Result<(Vec<String>, Vec<Vec<f64>>), String> {
    let mut d2_map = d2_map(reader, chain_id, format)?;
    let (_, values) = &mut d2_map;

    for line in values {
        for e in line {
            *e = e.sqrt();
        }
    }
    let d_map = d2_map;

    Ok(d_map)
}
