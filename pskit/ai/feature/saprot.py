import os
import warnings
import gc
import numpy as np
import torch
import json

from transformers import EsmTokenizer, EsmForMaskedLM

from ..utils import is_protein_chain, read_structure

warnings.filterwarnings("ignore")

torch.set_grad_enabled(False)


def first_protein_chain_id(pdb_file):
    structure = read_structure(pdb_file)
    model = structure[0]
    for chain in model.get_chains():
        if is_protein_chain(chain):
            return chain.id
    return None


def inference(model, input, save_path):
    with torch.no_grad():
        output = model(**input)
    output = output.logits.squeeze(0)
    output = output.detach().cpu().numpy()[1:-1, :]
    np.save(save_path, output)


def run(input_dir, output_dir, path):
    error = {}

    assert os.path.exists(path["foldseek"]), "Foldseek not found."
    assert os.path.exists(path["saprot_model"]), "SaProt model not found"

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    # Get structural seqs from pdb file
    def get_struc_seq(
        foldseek,
        pdb_path,
        chains: list = None,
        process_id: int = 0,
        plddt_path: str = None,
        plddt_threshold: float = 70.0,
    ) -> dict:
        """

        Args:
            foldseek: Binary executable file of foldseek
            path: Path to pdb file
            chains: Chains to be extracted from pdb file. If None, all chains will be extracted.
            process_id: Process ID for temporary files. This is used for parallel processing.
            plddt_path: Path to plddt file. If None, plddt will not be used.
            plddt_threshold: Threshold for plddt. If plddt is lower than this value, the structure will be masked.

        Returns:
            seq_dict: A dict of structural seqs. The keys are chain IDs. The values are tuples of
            (seq, struc_seq, combined_seq).
        """
        assert os.path.exists(foldseek), f"Foldseek not found: {foldseek}"
        assert os.path.exists(pdb_path), f"Pdb file not found: {pdb_path}"
        assert plddt_path is None or os.path.exists(plddt_path), f"Plddt file not found: {plddt_path}"
        prot_name = os.path.basename(pdb_path).split(".")[0]

        tmp_save_path = f"get_struc_seq_{prot_name}.tsv"
        cmd = f"{foldseek} structureto3didescriptor -v 0 --threads 1 --chain-name-mode 1 {pdb_path} {tmp_save_path}"
        os.system(cmd)

        seq_dict = {}
        name = os.path.basename(pdb_path)
        with open(tmp_save_path, "r") as r:
            for i, line in enumerate(r):
                desc, seq, struc_seq = line.split("\t")[:3]

                # Mask low plddt
                if plddt_path is not None:
                    with open(plddt_path, "r") as r:
                        plddts = np.array(json.load(r)["confidenceScore"])

                        # Mask regions with plddt < threshold
                        indices = np.where(plddts < plddt_threshold)[0]
                        np_seq = np.array(list(struc_seq))
                        np_seq[indices] = "#"
                        struc_seq = "".join(np_seq)

                name_chain = desc.split(" ")[0]
                chain = name_chain.replace(name, "").split("_")[-1]

                if chains is None or chain in chains:
                    if chain not in seq_dict:
                        combined_seq = "".join([a + b.lower() for a, b in zip(seq, struc_seq)])
                        seq_dict[chain] = (seq, struc_seq, combined_seq)

        os.remove(tmp_save_path)
        os.remove(tmp_save_path + ".dbtype")
        return seq_dict

    model_path = path["saprot_model"]
    tokenizer = EsmTokenizer.from_pretrained(model_path)
    model = EsmForMaskedLM.from_pretrained(model_path)
    model.to(device)
    model.eval()

    pdb_files = [os.path.join(input_dir, f) for f in os.listdir(input_dir) if f.endswith(".pdb") or f.endswith(".cif")]
    os.makedirs(output_dir, exist_ok=True)

    for pdb_file in pdb_files:
        try:
            prot_name, ext = os.path.splitext(os.path.basename(pdb_file))

            save_path = os.path.join(output_dir, prot_name + "_saprot.npy")
            if os.path.isfile(save_path):
                continue

            chain_id = first_protein_chain_id(pdb_file)
            if chain_id is None:
                raise Exception("Not having protein chain")

            parsed_seqs = get_struc_seq(path["foldseek"], pdb_file, chains=[chain_id])
            seq, foldseek_seq, combined_seq = parsed_seqs[chain_id]
            input = tokenizer(combined_seq, return_tensors="pt")

            inference(model, input.to(device), save_path)
        except Exception as e:
            error[os.path.basename(pdb_file)] = str(e)

    # 释放模型内存
    del model
    del tokenizer
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()

    return error
