import os
import traceback
import warnings

import torch
import torch.nn as nn
import esm
import fm

from .config import path

warnings.filterwarnings("ignore")


class ContrastiveLearningModel(nn.Module):
    def __init__(self, model_dim):
        super(ContrastiveLearningModel, self).__init__()
        self.nuc_adapter = nn.Sequential(nn.Linear(640, model_dim), nn.ReLU(), nn.LayerNorm(model_dim))  #
        self.prot_adapter = nn.Sequential(nn.Linear(640, model_dim), nn.ReLU(), nn.LayerNorm(model_dim))  #

    def forward(self, nuc_inputs, prot_inputs):

        nuc_vecs = nuc_inputs
        prot_vecs = prot_inputs

        nuc_vecs = self.nuc_adapter(nuc_vecs)  # [n, model_dim]
        prot_vecs = self.prot_adapter(prot_vecs)  # [m, model_dim]

        # [n,1,model_dim] [1,m,model_dim] -> [n,m]
        output = torch.cosine_similarity(torch.unsqueeze(nuc_vecs, 1), torch.unsqueeze(prot_vecs, 0), dim=2)

        return output


def rna_seq_embbding(seqs, device):
    model_path = path.get("rna-fm")
    if not os.path.exists(model_path):
        EmbbingModel, alphabet = fm.pretrained.rna_fm_t12()
    else:
        EmbbingModel, alphabet = fm.pretrained.fm.pretrained.rna_fm_t12(model_location=model_path)

    batch_converter = alphabet.get_batch_converter()
    EmbbingModel.to(device)
    EmbbingModel.eval()  # disables dropout for deterministic results

    tmp = []
    for i, seq in enumerate(seqs):
        batch_labels, batch_strs, batch_tokens = batch_converter([(i, seq)])
        with torch.no_grad():
            result = EmbbingModel(batch_tokens.to(device), repr_layers=[12])
            representation = result["representations"][12]
            tmp.append(representation[:, 1:-1, :].squeeze().mean(0))

    token_embeddings = torch.stack(tmp, dim=0)
    token_embeddings_cpu = token_embeddings.to("cpu")

    return token_embeddings_cpu


def prot_seq_embbding(seqs, device):
    model_path = path.get("esm2_150M")
    if not os.path.exists(model_path):
        EmbbingModel, alphabet = esm.pretrained.esm2_t30_150M_UR50D()
    else:
        EmbbingModel, alphabet = esm.pretrained.load_model_and_alphabet_local(model_path)
    batch_converter = alphabet.get_batch_converter()
    EmbbingModel.to(device)
    EmbbingModel.eval()  # disables dropout for deterministic results

    tmp = []
    for i, seq in enumerate(seqs):
        batch_labels, batch_strs, batch_tokens = batch_converter([(i, seq)])
        with torch.no_grad():
            result = EmbbingModel(batch_tokens.to(device), repr_layers=[30])
            representation = result["representations"][30]
            tmp.append(representation[:, 1:-1, :].squeeze().mean(0))

    token_embeddings = torch.stack(tmp, dim=0)
    token_embeddings_cpu = token_embeddings.to("cpu")

    return token_embeddings_cpu


def agent_run(protein_seq, nucleic_acid_seq, output_dir):
    device = torch.device("cpu")

    model_path = path.get("pair_model")
    if not os.path.exists(model_path):
        return f"Model file not found: {model_path}"

    model = ContrastiveLearningModel(model_dim=128)
    model.load_state_dict(torch.load(model_path, map_location=device))
    model.to(device)
    model.eval()

    try:
        rna_embbding = rna_seq_embbding([nucleic_acid_seq], device)
        prot_embbding = prot_seq_embbding([protein_seq], device)

        with torch.no_grad():
            output = model(rna_embbding, prot_embbding)
            output = torch.sigmoid(output)

        with open(os.path.join(output_dir, "PNI_predictions.csv"), "w") as f:
            f.write("Protein,Nucleic Acid,Binding Score\n")
            protein_seq_display = protein_seq if len(protein_seq) <= 20 else protein_seq[:10] + "..." + protein_seq[-10:]
            nucleic_acid_seq_display = nucleic_acid_seq if len(nucleic_acid_seq) <= 20 else nucleic_acid_seq[:10] + "..." + nucleic_acid_seq[-10:]
            f.write(f"{protein_seq_display},{nucleic_acid_seq_display},{round(output[0][0].item(), 3)}\n")

    except Exception as e:
        return str(e) + "\n" + traceback.format_exc()

    return f"result file: {os.path.join(output_dir, 'PNI_predictions.csv')}"


def main(paris, input_dir, output_dir):
    error = {}

    model_path = path.get("pair_model")
    if not os.path.exists(model_path):
        error["model"] = f"Model file not found: {model_path}"
        return error

    device = torch.device("cpu")

    model = ContrastiveLearningModel(model_dim=128)
    model.load_state_dict(torch.load(model_path, map_location=device))
    model.to(device)

    prot_seqs = [pair[0] for pair in paris]
    rna_seqs = [pair[1].upper().replace("T", "U") for pair in paris]
    try:
        rna_embbdings = rna_seq_embbding(rna_seqs, device)
        prot_embbdings = prot_seq_embbding(prot_seqs, device)

        with torch.no_grad():
            output = model(rna_embbdings, prot_embbdings)
            output = torch.sigmoid(output)

        with open(os.path.join(output_dir, "PNI_predictions.csv"), "w") as f:
            f.write("Protein,Nucleic Acid,Binding Score\n")
            for i in range(len(paris)):
                f.write(f"{prot_seqs[i]},{rna_seqs[i]},{round(output[i][i].item(), 3)}\n")
    except Exception as e:
        error["prediction"] = str(e) + "\n" + traceback.format_exc()

    return error


if __name__ == "__main__":
    # for agent_run
    import argparse

    parser = argparse.ArgumentParser(description="Run PAIR agent")
    parser.add_argument("--protein_seq", type=str, required=True, help="Protein sequence")
    parser.add_argument("--nucleic_acid_seq", type=str, required=True, help="Nucleic acid sequence")
    parser.add_argument("--output_dir", type=str, required=True, help="Directory to save output")
    args = parser.parse_args()

    result = agent_run(args.protein_seq, args.nucleic_acid_seq, args.output_dir)
    print(result)
