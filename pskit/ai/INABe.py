from html import parser
import pickle
import os
import traceback
import shutil

import torch

from .feature.extract import run as extract_feats
from .config import path, inabe_model
from .model.INAB import INAB
from .utils import read_structure, is_protein_chain, has_protein_chain

torch.set_grad_enabled(False)


def get_seq(pdb_file):
    prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
    structure = read_structure(pdb_file)
    model = structure[0]
    seq = []
    chain_id = None
    for chain in model.get_chains():
        if is_protein_chain(chain):
            chain_id = chain.id
            for residue in chain.get_residues():
                if residue.id[0] == " ":
                    seq.append(f"{chain_id},{residue.id[1]},{residue.resname}")
            break  # Only consider the first protein chain

    if chain_id is None:
        raise Exception(f"No protein chain found in {prot_name}")
    else:
        return seq, chain_id


def agent_run(pdb_file, output_dir, target_type):
    device = torch.device("cpu")
    model = INAB(inabe_model)

    if target_type == "DNA":
        model_path = path.get("inabe_dna")
    else:
        model_path = path.get("inabe_rna")

    if not model_path or not os.path.exists(model_path):
        return f"Model file not found: {model_path}"

    model.load_state_dict(torch.load(model_path, map_location=torch.device("cpu"), weights_only=True))
    model.eval()
    model.to(device)

    if not has_protein_chain(read_structure(pdb_file)):
        return f"{pdb_file}: Not having protein chain"
    elif len(get_seq(pdb_file)[0]) > 1000:
        return f"{pdb_file}: Sequence length exceeds 1000 residues"

    success_feature_pdbs, error_feature_pdbs = extract_feats([pdb_file], output_dir, path)
    if not success_feature_pdbs:
        return f"{pdb_file}: Feature extraction failed: {error_feature_pdbs.get(os.path.basename(pdb_file), 'Unknown error')}"

    os.makedirs(output_dir, exist_ok=True)

    for pdb_file in success_feature_pdbs:
        prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
        try:
            seq, chain_id = get_seq(pdb_file)
            pkl_file = os.path.join(output_dir, prot_name + "_input.pkl")

            if not os.path.exists(pkl_file):
                return f"{pdb_file}: Feature file not found: {pkl_file}"

            with open(pkl_file, "rb") as f:
                input_data = pickle.load(f)

            prot_name_pkl, node_feats, coords, edges, edge_attr = input_data

            coords = torch.from_numpy(coords).to(device).float()
            edges = torch.from_numpy(edges).to(device).long()
            edge_attr = torch.from_numpy(edge_attr).to(device).float()
            node_feats = torch.from_numpy(node_feats).to(device).float()

            with torch.no_grad():
                output = model(node_feats, coords, edges, edge_attr)

            output = output.squeeze().cpu().tolist()

            output_file = os.path.join(output_dir, f"{prot_name}_binding_sites.csv")
            with open(output_file, "w") as f:
                f.write("chain,residue_number,residue_name,score\n")
                for res_name, score in zip(seq, output):
                    f.write(f"{res_name},{round(score, 4)}\n")

        except Exception as e:
            return f"{pdb_file}: {str(e)}\n{traceback.format_exc()}"

    return f"result file: {os.path.join(output_dir, f'{prot_name}_binding_sites.csv')}"


def main(input_dir, output_dir, target_type):
    error = {}

    device = torch.device("cpu")
    model = INAB(inabe_model)

    if target_type == "DNA":
        model_path = path.get("inabe_dna")
    else:
        model_path = path.get("inabe_rna")

    if not model_path or not os.path.exists(model_path):
        error["model"] = f"Model file not found: {model_path}"
        return error

    model.load_state_dict(torch.load(model_path, map_location=torch.device("cpu"), weights_only=True))
    model.eval()
    model.to(device)

    pdb_files = [os.path.join(input_dir, f) for f in os.listdir(input_dir) if f.endswith(".pdb") or f.endswith(".cif")]

    if not pdb_files:
        error["input"] = "No PDB/CIF files found in input directory"
        return error

    for pdb_file in pdb_files:
        try:
            prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
            if not has_protein_chain(read_structure(pdb_file)):
                error[prot_name + ext] = "Not having protein chain"
            elif len(get_seq(pdb_file)[0]) > 1000:
                error[prot_name + ext] = "Sequence length exceeds 1000 residues"
        except Exception as e:
            error[prot_name + ext] = str(e) + "\n" + traceback.format_exc()

    pdb_files = [f for f in pdb_files if os.path.splitext(os.path.basename(f))[0] not in error]
    if pdb_files == []:
        return error

    feats_output_dir = input_dir  # Save features in the same directory as input structures
    try:
        success_feature_pdbs, error_feature_pdbs = extract_feats(pdb_files, feats_output_dir, path)
        error.update(error_feature_pdbs)
    except Exception as e:
        error["feature_extraction"] = str(e) + "\n" + traceback.format_exc()
        return error

    os.makedirs(output_dir, exist_ok=True)

    for pdb_file in success_feature_pdbs:
        prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
        try:
            seq, chain_id = get_seq(pdb_file)
            pkl_file = os.path.join(feats_output_dir, prot_name + "_input.pkl")

            if not os.path.exists(pkl_file):
                error[prot_name + ext] = f"Feature file not found: {pkl_file}"
                continue

            with open(pkl_file, "rb") as f:
                input_data = pickle.load(f)

            prot_name_pkl, node_feats, coords, edges, edge_attr = input_data

            coords = torch.from_numpy(coords).to(device).float()
            edges = torch.from_numpy(edges).to(device).long()
            edge_attr = torch.from_numpy(edge_attr).to(device).float()
            node_feats = torch.from_numpy(node_feats).to(device).float()

            with torch.no_grad():
                output = model(node_feats, coords, edges, edge_attr)

            output = output.squeeze().cpu().tolist()

            output_file = os.path.join(output_dir, f"{prot_name}_binding_sites.csv")
            with open(output_file, "w") as f:
                f.write("chain,residue_number,residue_name,score\n")
                for res_name, score in zip(seq, output):
                    f.write(f"{res_name},{round(score, 4)}\n")

            # Copy input structure file to output directory for visualization
            output_structure = os.path.join(output_dir, os.path.basename(pdb_file))
            if not os.path.exists(output_structure):
                shutil.copy2(pdb_file, output_structure)

        except Exception as e:
            error[prot_name + ext] = str(e) + "\n" + traceback.format_exc()

    return error


if "__main__" == __name__:
    # for agent_run
    import argparse

    parser = argparse.ArgumentParser(description="Run INABe agent")
    parser.add_argument("--pdb_path", type=str, required=True, help="Path to input PDB file")
    parser.add_argument("--output_dir", type=str, required=True, help="Directory to save output")
    parser.add_argument("--ligand_type", type=str, required=True, choices=["DNA", "RNA"], help="Type of nucleic acid target")
    args = parser.parse_args()

    result = agent_run(args.pdb_path, args.output_dir, args.ligand_type)
    print(result)
