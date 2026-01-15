import os

base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

inabe_model = {
    "d_model": 256,
    "seq_model": "transformer",
    "num_seq_model_layers": 1,
    "num_struc_model_layers": 3,
    "feats": "esm2_saprot",
    "mode": "regression",
    "no_cross_attention": False,
    "order": "seq_struc",
}

path = {
    "base_dir": base_dir,
    "foldseek": "/usr/local/bin/foldseek",
    "dssp": "/usr/local/bin/mkdssp",
    "saprot_model": os.path.join(base_dir, "model_parameters", "SaProt_650M_PDB"),
    "esm2_model": os.path.join(base_dir, "model_parameters", "esm2_650M", "esm2_t33_650M_UR50D.pt"),
    "inabe_dna": os.path.join(base_dir, "model_parameters", "INABe_DNA.pth"),
    "inabe_rna": os.path.join(base_dir, "model_parameters", "INABe_RNA.pth"),
}
