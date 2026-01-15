import os
import warnings
import gc

import numpy as np
import torch
import esm

from ..utils import read_structure, is_protein_chain

warnings.filterwarnings("ignore")

torch.set_grad_enabled(False)


def get_seq(pdb_file):
    AA_dic = {
        "GLY": "G",
        "ALA": "A",
        "VAL": "V",
        "LEU": "L",
        "ILE": "I",
        "PHE": "F",
        "TRP": "W",
        "TYR": "Y",
        "ASP": "D",
        "ASN": "N",
        "GLU": "E",
        "LYS": "K",
        "GLN": "Q",
        "MET": "M",
        "SER": "S",
        "THR": "T",
        "CYS": "C",
        "PRO": "P",
        "HIS": "H",
        "ARG": "R",
    }
    prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
    structure = read_structure(pdb_file)
    model = structure[0]
    seq = ""
    for chain in model.get_chains():
        if is_protein_chain(chain):
            for residue in chain.get_residues():
                if residue.id[0] == " ":
                    seq += AA_dic[residue.resname]
            break  # Only consider the first protein chain

    save_path = f"{os.path.join(os.path.dirname(pdb_file), prot_name + '.fasta')}"

    with open(save_path, "w") as f:
        f.write(f">{prot_name}\n{seq}\n")


def inference(model, input, save_path):
    with torch.no_grad():
        results = model(input, repr_layers=[33], return_contacts=True)

    token_representations = results["representations"][33]
    prot_rep = token_representations[:, 1:-1, :].squeeze()

    rep_numpy = prot_rep.detach().cpu().numpy()
    np.save(save_path, rep_numpy)


def run(input_dir, output_dir, path):
    error = {}

    device = torch.device("cpu")
    esm2_path = path.get("esm2_model", None)
    if esm2_path is not None and os.path.exists(esm2_path):
        model, alphabet = esm.pretrained.load_model_and_alphabet_local(esm2_path)
    else:
        model, alphabet = esm.pretrained.esm2_t33_650M_UR50D()
    model.to(device)
    model.eval()
    batch_converter = alphabet.get_batch_converter()

    pdb_files = [os.path.join(input_dir, f) for f in os.listdir(input_dir) if f.endswith(".pdb") or f.endswith(".cif")]
    os.makedirs(output_dir, exist_ok=True)

    for pdb_file in pdb_files:
        try:
            prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
            save_path = os.path.join(output_dir, prot_name + "_esm2.npy")
            if os.path.isfile(save_path):
                continue

            fasta_file = os.path.join(input_dir, prot_name + ".fasta")
            if not os.path.isfile(fasta_file):
                get_seq(pdb_file)

            with open(fasta_file) as f:
                f.readline()
                seq = f.readline().strip()

            data = [(prot_name, seq)]
            batch_labels, batch_strs, batch_tokens = batch_converter(data)
            # batch_lens = (batch_tokens != alphabet.padding_idx).sum(1)
            batch_tokens_device = batch_tokens.to(device)

            inference(model, batch_tokens_device, save_path)
        except Exception as e:
            error[os.path.basename(pdb_file)] = str(e)

    # 释放模型内存
    del model
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()

    return error
