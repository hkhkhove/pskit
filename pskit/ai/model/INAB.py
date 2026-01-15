from torch import nn
from .egnn_clean import EGNN


# feat_dim
# hhm(30): 0-29
# pssm(20): 30-49
# ss(14): 50-63
# af(7): 64-70
# esm2_rep(1280): 71-1350
# gearnet_rep(512): 1351-1862
# saprot_rep(446): 1863-2308
feats_dim = {
    "all": 2309,
    "no_hhm": 2279,
    "no_pssm": 2289,
    "no_hhm_pssm": 2259,
    "no_ss": 2295,
    "no_af": 2302,
    "no_ss_af": 2288,
    "no_esm2": 1029,
    "no_gearnet": 1797,
    "no_saprot": 1863,
    "no_plm": 71,
    "no_empirical": 2238,
    "no_gearnet_saprot": 1351,
    "no_esm2_saprot": 583,
    "no_esm2_gearnet": 517,
    "no_hhm_pssm_esm2_gearnet": 467,
    "esm2_saprot": 1726,
}


class INAB(nn.Module):
    def __init__(self, config):
        super().__init__()
        self.config = config
        self.embedding = nn.Linear(feats_dim[config["feats"]], config["d_model"])

        if config["seq_model"] == "transformer":
            encoder_layer = nn.TransformerEncoderLayer(d_model=config["d_model"], nhead=1, dropout=0.1, batch_first=True)
            self.seq_model = nn.TransformerEncoder(encoder_layer, num_layers=config["num_seq_model_layers"])
        else:
            raise ValueError("seq_model should be either mamba or transformer")

        self.struc_model = EGNN(
            in_node_nf=config["d_model"],
            hidden_nf=config["d_model"],
            out_node_nf=config["d_model"],
            in_edge_nf=1,
            n_layers=config["num_struc_model_layers"],
            attention=True,
        )

        self.cross_attention = nn.MultiheadAttention(embed_dim=config["d_model"], num_heads=1)
        self.W_Q = nn.Linear(config["d_model"], config["d_model"])
        self.W_K = nn.Linear(config["d_model"], config["d_model"])
        self.W_V = nn.Linear(config["d_model"], config["d_model"])

        self.predictor = nn.Sequential(
            nn.Linear(config["d_model"], config["d_model"] // 2),
            nn.LayerNorm(config["d_model"] // 2),
            nn.SiLU(),
            nn.Linear(config["d_model"] // 2, config["d_model"] // 4),
            nn.LayerNorm(config["d_model"] // 4),
            nn.SiLU(),
            nn.Linear(config["d_model"] // 4, 1),
        )

    def forward(self, node_feats, coords, edges, edge_attr):
        h = self.embedding(node_feats).unsqueeze(dim=0)  # (batch,seq_len,feat_dim), batch=1

        seq_h = self.seq_model(h)

        struc_h, x = self.struc_model(h.squeeze(0), coords, edges, edge_attr)  # (batch,seq_len,feat_dim)
        struc_h = struc_h.unsqueeze(dim=0)  # (batch,seq_len,feat_dim), batch=1
        if not self.config["no_cross_attention"]:
            if self.config["order"] == "seq_struc":
                Q = self.W_Q(seq_h)
                K = self.W_K(struc_h)
                V = self.W_V(struc_h)
            else:
                Q = self.W_Q(struc_h)
                K = self.W_K(seq_h)
                V = self.W_V(seq_h)

            fused_h, weights = self.cross_attention(Q, K, V)
        else:
            fused_h = (seq_h + struc_h) / 2

        y = self.predictor(fused_h)

        return y
