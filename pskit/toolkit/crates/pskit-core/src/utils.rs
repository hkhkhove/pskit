use pdbtbx::{Format, PDBError, PDB};
use std::io::{BufRead, BufReader, BufWriter};

pub fn three_to_one(three: &str) -> char {
    match three {
        "ALA" => 'A',
        "ARG" => 'R',
        "ASN" => 'N',
        "ASP" => 'D',
        "CYS" => 'C',
        "GLN" => 'Q',
        "GLU" => 'E',
        "GLY" => 'G',
        "HIS" => 'H',
        "ILE" => 'I',
        "LEU" => 'L',
        "LYS" => 'K',
        "MET" => 'M',
        "PHE" => 'F',
        "PRO" => 'P',
        "SER" => 'S',
        "THR" => 'T',
        "TRP" => 'W',
        "TYR" => 'Y',
        "VAL" => 'V',
        "UNK" => 'X',
        _ => 'X',
    }
}

pub const PROTEIN_RESIDUES: &[&str] = &[
    "ALA", "ARG", "ASN", "ASP", "CYS", "GLN", "GLU", "GLY", "HIS", "ILE", "LEU", "LYS", "MET",
    "PHE", "PRO", "SER", "THR", "TRP", "TYR", "VAL",
];
pub const NUCLEIC_RESIDUES: &[&str] = &["A", "C", "G", "U", "DA", "DC", "DG", "DT"];

pub fn is_protein_residue(name: &str) -> bool {
    PROTEIN_RESIDUES.contains(&name)
}

pub fn is_nucleic_residue(name: &str) -> bool {
    NUCLEIC_RESIDUES.contains(&name)
}

pub fn read_raw<R: BufRead>(reader: R, format: &str) -> Result<(PDB, Vec<PDBError>), String> {
    let buf = BufReader::new(reader);
    let format = match format.to_uppercase().as_str() {
        "PDB" => Format::Pdb,
        "CIF" => Format::Mmcif,
        _ => Format::Auto,
    };
    let (pdb, errors) = pdbtbx::ReadOptions::default()
        .set_format(format)
        .set_level(pdbtbx::StrictnessLevel::Loose)
        .set_only_first_model(true)
        .read_raw(buf)
        .map_err(|errs| {
            let true_errs: Vec<_> = errs
                .into_iter()
                .filter(|err| err.fails(pdbtbx::StrictnessLevel::Loose))
                .collect();
            format!("File parsing failed due to: {:?}", true_errs[0])
        })?;

    Ok((pdb, errors))
}

pub fn write_raw(pdb: PDB, format: &str) -> Vec<u8> {
    let mut pdb_bytes: Vec<u8> = Vec::new();
    let format = match format.to_uppercase().as_str() {
        "PDB" => Format::Pdb,
        "CIF" => Format::Mmcif,
        _ => Format::Auto,
    };
    let sink = BufWriter::new(&mut pdb_bytes);
    match format {
        Format::Pdb => {
            pdbtbx::save_pdb_raw(&pdb, sink, pdbtbx::StrictnessLevel::Loose);
        }
        _ => {
            pdbtbx::save_mmcif_raw(&pdb, sink);
        }
    }
    pdb_bytes
}
