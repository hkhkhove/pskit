use pdbtbx::{
    ContainsAtomConformer, ContainsAtomConformerResidue, ContainsAtomConformerResidueChain,
};
use rstar::{primitives::GeomWithData, RTree};
use std::collections::HashMap;
use std::io::BufRead;

use crate::utils::{is_nucleic_residue, is_protein_residue, read_raw};

#[derive(Clone, Debug)]
struct ResidueId {
    chain_id: String,
    resseq: isize,
    resname: String,
}

fn key_for_sort(s: &str) -> (&str, isize, &str, isize, &str) {
    let front = s.split('_').next().unwrap_or("");
    let mut parts = front.split('-');

    let a_cid = parts.next().unwrap_or("\u{10FFFF}");
    let a_resseq = parts
        .next()
        .and_then(|s| s.parse::<isize>().ok())
        .unwrap_or(isize::MAX);
    let _ = parts.next();
    let b_cid = parts.next().unwrap_or("\u{10FFFF}");
    let b_resseq = parts
        .next()
        .and_then(|s| s.parse::<isize>().ok())
        .unwrap_or(isize::MAX);

    (a_cid, a_resseq, b_cid, b_resseq, s)
}

pub fn compute_binding_pairs<R: BufRead>(
    reader: R,
    cutoff: f64,
    format: &str,
) -> Result<Vec<(String, f64)>, String> {
    let (pdb, _errors) = read_raw(reader, format)?;

    let mut protein_has = false;
    let mut nucleic_has = false;
    'scan: for chain in pdb.chains() {
        for residue in chain.residues() {
            if let Some(name) = residue.name() {
                protein_has |= is_protein_residue(name);
                nucleic_has |= is_nucleic_residue(name);
                if protein_has && nucleic_has {
                    break 'scan;
                }
            }
        }
    }

    if !(protein_has && nucleic_has) {
        return Err("Not a protein-nucleic acid complex.".to_string());
    }

    // 只对核酸原子建 rtree
    let mut nuc_points: Vec<GeomWithData<[f64; 3], ResidueId>> = Vec::new();
    for atom in pdb.atoms_with_hierarchy() {
        let resname = match atom.residue().name() {
            Some(n) => n,
            None => continue,
        };
        if !is_nucleic_residue(resname) {
            continue;
        }
        let (x, y, z) = atom.atom().pos();
        nuc_points.push(GeomWithData::new(
            [x, y, z],
            ResidueId {
                chain_id: atom.chain().id().to_string(),
                resseq: atom.residue().id().0,
                resname: resname.to_string(),
            },
        ));
    }
    let nuc_tree: RTree<GeomWithData<[f64; 3], ResidueId>> = RTree::bulk_load(nuc_points);

    let mut pairs: HashMap<String, f64> = HashMap::new();
    for atom_a in pdb.atoms_with_hierarchy() {
        let a_resname = match atom_a.residue().name() {
            Some(n) => n,
            None => continue,
        };
        if !is_protein_residue(a_resname) {
            continue;
        }

        let ap = atom_a.atom().pos();
        let a_name = format!(
            "{}-{}-{}",
            atom_a.chain().id(),
            atom_a.residue().id().0,
            a_resname
        );

        for b in nuc_tree.locate_within_distance([ap.0, ap.1, ap.2], cutoff * cutoff) {
            let bp = b.geom();
            let dx = ap.0 - bp[0];
            let dy = ap.1 - bp[1];
            let dz = ap.2 - bp[2];
            let d = dx * dx + dy * dy + dz * dz;

            let b_res = &b.data;
            let pair = format!(
                "{a_name}_{}-{}-{}",
                b_res.chain_id, b_res.resseq, b_res.resname
            );

            pairs
                .entry(pair)
                .and_modify(|v| {
                    if d < *v {
                        *v = d;
                    }
                })
                .or_insert(d);
        }
    }

    for v in pairs.values_mut() {
        *v = v.sqrt();
    }

    let mut pairs: Vec<_> = pairs.into_iter().collect();
    pairs.sort_by(|ka, kb| {
        let a = key_for_sort(&ka.0);
        let b = key_for_sort(&kb.0);

        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.4.cmp(&b.4))
    });

    Ok(pairs)
}
