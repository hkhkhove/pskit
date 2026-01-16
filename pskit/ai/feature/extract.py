from .esm2 import run as extract_ESM2_feats
from .saprot import run as extract_SaProt_feats
from ..utils import is_protein_chain

import os
import warnings
import pickle

import numpy as np
from Bio.PDB.PDBParser import PDBParser
from Bio.PDB.MMCIFParser import MMCIFParser

warnings.filterwarnings("ignore")


def get_edges(pdb_file, cutoff=12):
    prot_name, ext = os.path.splitext(os.path.basename(pdb_file))
    if ext == ".cif":
        parser = MMCIFParser()
    else:
        parser = PDBParser()
    structure = parser.get_structure(prot_name, pdb_file)
    edges = []
    edge_attr = []
    left = []
    right = []
    coords = []
    for chain in structure[0]:
        if is_protein_chain(chain):
            for residue in chain:
                if residue.id[0] == " ":
                    atom_coords = [atom.coord for atom in residue]
                    coord = np.mean(atom_coords, axis=0)
                    coords.append(coord)
            break  # Only consider the first protein chain

    for i, coord_i in enumerate(coords):
        for j, coord_j in enumerate(coords):
            if j <= i:
                continue
            diff_vector = coord_i - coord_j
            d2 = np.sum(diff_vector * diff_vector)
            if d2 is not None and d2 <= cutoff * cutoff:
                left.append(i)
                right.append(j)
                left.append(j)
                right.append(i)
                weight = np.log(abs(i - j)) / np.sqrt(d2)
                edge_attr.append([weight])
                edge_attr.append([weight])

    edges.append(left)
    edges.append(right)
    return np.array(edges), np.array(edge_attr), np.array(coords)


def normalize_np_array(np_array, p=2, axis=0):
    # L2 norm
    norm = np.linalg.norm(np_array, ord=p, axis=axis, keepdims=True)
    normalized_array = np_array / norm
    return normalized_array


def standardization(data, epsilon=1e-8):
    mu = np.mean(data, axis=0, keepdims=True)
    sigma = np.std(data, axis=0, keepdims=True)
    sigma = np.where(sigma < epsilon, 1.0, sigma)
    return (data - mu) / sigma


def combine(pdb_files):
    success_combine = []
    error_combine = {}

    for pdb_file in pdb_files:
        try:
            prot_name, ext = os.path.splitext(os.path.basename(pdb_file))

            esm2_file = pdb_file.replace(ext, "_esm2.npy")
            saprot_file = pdb_file.replace(ext, "_saprot.npy")

            if not os.path.exists(esm2_file):
                error_combine[prot_name + ext] = "Extract ESM2 feature failed"
                continue
            if not os.path.exists(saprot_file):
                error_combine[prot_name + ext] = "Extract SaProt feature failed"
                continue

            esm2_rep = np.load(esm2_file)
            saprot_rep = np.load(saprot_file)

            esm2_rep = normalize_np_array(esm2_rep)
            saprot_rep = normalize_np_array(saprot_rep)

            edges, edge_attr, coords = get_edges(pdb_file)

            assert esm2_rep.shape[0] == saprot_rep.shape[0] == len(coords), f"feature dimension mismatch, esm2_rep:{esm2_rep.shape[0]},saprot_rep:{saprot_rep.shape[0]},coords:{len(coords)}"

            node_feats = np.concatenate([esm2_rep, saprot_rep], axis=1)

            input_data_file = pdb_file.replace(ext, "_input.pkl")

            with open(input_data_file, "wb") as f:
                pickle.dump((prot_name, node_feats, coords, edges, edge_attr), f)

            success_combine.append(pdb_file)
        except Exception as e:
            error_combine[prot_name + ext] = f"Prepare feature failed due to {str(e)}"

    return success_combine, error_combine


def run(pdb_files, path):
    """
    Extract features for binding site prediction.

    Args:
        pdb_files: List of PDB/CIF file paths
        path: Dictionary containing paths to models

    Returns:
        List of successfully processed PDB file paths
    """

    # Get input directory from first file
    input_dir = os.path.dirname(pdb_files[0])

    extract_ESM2_feats(input_dir, input_dir, path)
    extract_SaProt_feats(input_dir, input_dir, path)
    success_combine, error_combine = combine(pdb_files)

    return success_combine, error_combine
