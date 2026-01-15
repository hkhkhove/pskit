import os
from Bio.PDB import PDBList, PDBParser, MMCIFParser

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
AA = [
    "GLY",
    "ALA",
    "VAL",
    "LEU",
    "ILE",
    "PHE",
    "TRP",
    "TYR",
    "ASP",
    "ASN",
    "GLU",
    "LYS",
    "GLN",
    "MET",
    "SER",
    "THR",
    "CYS",
    "PRO",
    "HIS",
    "ARG",
]
NA = ["DA", "DC", "DT", "DG", "A", "C", "T", "U", "G"]


def download_pdb(prot_list, save_path):
    success = []
    error = {}
    pdbl = PDBList(verbose=False)
    for prot in prot_list:
        try:
            pdbl.retrieve_pdb_file(prot, file_format="mmCif", pdir=save_path)
            if not os.path.isfile(os.path.join(save_path, f"{prot}.cif")):
                raise Exception(f"Download structure with id '{prot}' from pdb server failed")
            else:
                success.append(prot)
        except Exception as e:
            error[prot] = str(e)

    return success, error


def read_structure(pdb_file):
    structure = None
    filename, ext = os.path.splitext(os.path.basename(pdb_file))
    if ext.lower() == ".cif":
        current_parser = MMCIFParser(QUIET=True)
    elif ext.lower() == ".pdb":
        current_parser = PDBParser(QUIET=True)
    else:
        raise Exception(f"Unsupported file format: {ext}")

    try:
        structure = current_parser.get_structure(filename, pdb_file)
    except Exception:
        raise Exception(f"Failed to read structure from {os.path.basename(pdb_file)}")

    return structure


def is_pna_complex(structure):
    has_protein = False
    has_nucleic_acid = False
    for model in structure:
        for chain in model:
            for residue in chain:
                if residue.id[0] == " ":
                    if residue.resname in AA:
                        has_protein = True
                    if residue.resname in NA:
                        has_nucleic_acid = True
                    if has_protein and has_nucleic_acid:
                        return True
    return False


def is_protein(structure):
    for model in structure:
        for chain in model:
            for residue in chain:
                if residue.id[0] == " ":
                    if residue.resname not in AA:
                        return False
    return True

def is_protein_chain(chain):
    for residue in chain:
        if residue.id[0] == " ":
            if residue.resname not in AA:
                return False
    return True

def has_protein_chain(structure):
    for model in structure:
        for chain in model:
            if is_protein_chain(chain):
                return True
    return False