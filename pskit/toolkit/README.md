
## pskit-cli

`pskit-cli` is a Rust command-line tool built on top of `pskit-core`.

Build:

```bash
cargo build -p pskit-cli
```

Common usage:

```bash
# Split by chain
cargo run -p pskit-cli -- split-by-chain -i crates/pskit-core/test_pdbs/7U5E.cif -F cif -o /tmp/chains

# Split protein / nucleic-acid complex into Prot + NA
cargo run -p pskit-cli -- split-complex -i crates/pskit-core/test_pdbs/7U5E.cif -F cif -o /tmp/complex_parts

# Extract fragment
cargo run -p pskit-cli -- extract-fragment -i crates/pskit-core/test_pdbs/7U5E.cif -F cif -c A --start 10 --end 60 -o /tmp/frag.cif

# Contact map (d / d2 / knn)
cargo run -p pskit-cli -- contact-map -i crates/pskit-core/test_pdbs/7U5E.cif -F cif -m d -o /tmp/contact.json

# Binding-pair annotation TSV
cargo run -p pskit-cli -- annotate-binding-pairs -i crates/pskit-core/test_pdbs/7U5E.cif -F cif --cutoff 3.5 -o /tmp/pairs.tsv
```