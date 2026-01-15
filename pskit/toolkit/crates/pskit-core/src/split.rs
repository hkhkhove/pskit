use crate::utils::{is_nucleic_residue, is_protein_residue, read_raw, write_raw};
use pdbtbx::{Model, PDB};
use std::collections::{BTreeSet, HashMap};
use std::io::BufRead;

pub fn split_by_chain<R: BufRead>(
    reader: R,
    format: &str,
) -> Result<HashMap<String, Vec<u8>>, String> {
    let (pdb, _errors) = read_raw(reader, format)?;

    let mut chain_ids = BTreeSet::new();

    for model in pdb.models() {
        for chain in model.chains() {
            chain_ids.insert(chain.id().to_string());
        }
    }

    let mut chains = HashMap::with_capacity(chain_ids.len());

    for chain_id in chain_ids {
        let mut out = PDB::new();
        // 可选：拷贝元数据（保留 header/对称/晶胞等信息）
        out.identifier = pdb.identifier.clone();
        out.scale = pdb.scale.clone();
        out.origx = pdb.origx.clone();
        out.unit_cell = pdb.unit_cell.clone();
        out.symmetry = pdb.symmetry.clone();
        // 复制 MTRIX
        for m in pdb.mtrix() {
            out.add_mtrix(m.clone());
        }
        // 复制 REMARK
        for (ty, text) in pdb.remarks() {
            let _ = out.add_remark(*ty, text.clone());
        }

        for model in pdb.models() {
            let mut new_model = Model::new(model.serial_number());
            if let Some(ch) = model.chains().find(|c| c.id() == chain_id) {
                new_model.add_chain(ch.clone());
                out.add_model(new_model);
            }
        }

        if out.model_count() == 0 {
            continue;
        }

        chains.insert(chain_id, write_raw(out, format));
    }

    Ok(chains)
}

pub fn split_complex<R: BufRead>(
    reader: R,
    format: &str,
) -> Result<HashMap<String, Vec<u8>>, String> {
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

    let mut parts: HashMap<String, Vec<u8>> = HashMap::with_capacity(2);

    let mut protein_pdb = pdb.clone();
    let mut nucleic_pdb = pdb.clone();

    protein_pdb.remove_residues_by(|residue| match residue.name() {
        Some(name) => !is_protein_residue(name),
        None => true,
    });
    nucleic_pdb.remove_residues_by(|residue| match residue.name() {
        Some(name) => !is_nucleic_residue(name),
        None => true,
    });

    let protein_bytes = write_raw(protein_pdb, format);
    let nucleic_bytes = write_raw(nucleic_pdb, format);

    parts.insert("Prot".to_string(), protein_bytes);
    parts.insert("NA".to_string(), nucleic_bytes);

    Ok(parts)
}

pub fn extract_fragment<R: BufRead>(
    reader: R,
    chain_id: String,
    requested_start: Option<isize>,
    requested_end: Option<isize>,
    format: &str,
) -> Result<(Vec<u8>, isize, isize), String> {
    let (mut pdb, _errors) = read_raw(reader, format)?;

    let chain_ids: Vec<_> = pdb.chains().map(|chain| chain.id()).collect();
    if !chain_ids.contains(&chain_id.as_str()) {
        return Err(format!(
            "Chain {chain_id} not exists. Valid chain IDs are: {:?}",
            chain_ids
        ));
    }

    let mut actual_start = 0;
    let mut actual_end = 0;

    for chain in pdb.chains_mut() {
        if chain.id() != chain_id {
            chain.remove_residues_by(|_| true);
            continue;
        }
        let mut fragment_start = if let Some(residue) = chain.residues().next() {
            residue.id().0
        } else {
            0
        };
        let mut frgment_end = if let Some(residue) = chain.residues().next_back() {
            residue.id().0
        } else {
            0
        };

        match (requested_start, requested_end) {
            (Some(start), Some(end)) => {
                if start < fragment_start || end > frgment_end {
                    return Err(format!("Invalid range. For Chain {chain_id}, please enter values between {fragment_start} and {frgment_end}."));
                } else {
                    fragment_start = start;
                    frgment_end = end;
                }
            }
            (Some(start), None) => {
                if start < fragment_start {
                    return Err(format!("Invalid range. For Chain {chain_id}, please enter values between {fragment_start} and {frgment_end}."));
                } else {
                    fragment_start = start;
                }
            }
            (None, Some(end)) => {
                if end > frgment_end {
                    return Err(format!("Invalid range. For Chain {chain_id}, please enter values between {fragment_start} and {frgment_end}."));
                } else {
                    frgment_end = end;
                }
            }
            (None, None) => {}
        }

        actual_start = fragment_start;
        actual_end = frgment_end;

        chain.remove_residues_by(|residue| {
            residue.id().0 < fragment_start || residue.id().0 > frgment_end
        });
    }

    Ok((write_raw(pdb, format), actual_start, actual_end))
}
