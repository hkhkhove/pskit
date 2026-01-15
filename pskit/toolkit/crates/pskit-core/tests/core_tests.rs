// use pskit_core::annotate;
use pskit_core::annotate::compute_binding_pairs;
use pskit_core::contact::d2_map;
use pskit_core::split::extract_fragment;
use pskit_core::split::split_complex;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn test_binding_pairs() {
        use std::fs::File;
        let pdb_path = "./test_pdbs/7U5E.cif";
        let reader = BufReader::new(File::open(pdb_path).expect("open file"));
        let pairs = compute_binding_pairs(reader, 3.5, "cif").expect("compute_pairs");
        println!("{pairs:?}");
    }
    #[test]
    fn test_extract_fragment() {
        use std::fs::File;
        let pdb_path = "./test_pdbs/7U5E.cif";
        let reader = BufReader::new(File::open(pdb_path).expect("open file"));
        let (frag_bytes, start, end) =
            extract_fragment(reader, "A".to_string(), None, None, "cif").unwrap();
        println!("{:?}\n{start}-{end}", std::str::from_utf8(&frag_bytes));
    }
    #[test]
    fn test_split() {
        use std::fs::File;
        let pdb_path = "./test_pdbs/7U5E.cif";
        let reader = BufReader::new(File::open(pdb_path).expect("open file"));
        let parts = split_complex(reader, "cif").unwrap();
        for part in parts {
            println!(
                "========{}========\n{:?}",
                part.0,
                std::str::from_utf8(&part.1).unwrap()
            );
        }
    }

    #[test]
    fn test_pdbtbx() {
        use std::fs::File;
        let pdb_path = "./test_pdbs/8W2S.cif";
        let reader = BufReader::new(File::open(pdb_path).unwrap());
        let d2_map = d2_map(reader, Some("A".into()), "cif");
        println!("{:?}", d2_map);
    }
}
