from dataclasses import dataclass
import json
import os
import sys
import traceback

from .config import path
from .feature import empirical_feats, esm2, saprot
from .utils import download_pdb
from .INABe import predict


@dataclass
class BaseParams:
    task_id: str = ""
    task_name: str = ""
    input_method: str = ""
    ids: str = ""
    input_dir: str = ""
    output_dir: str = ""


@dataclass
class PredBS(BaseParams):
    ligand_type: str = "DNA"

    def run(self):
        return predict(
            input_dir=self.input_dir,
            output_dir=self.output_dir,
            target_type=self.ligand_type,
        )


@dataclass
class EmpFeats(BaseParams):
    emp_feats: str = "dssp"  # 逗号分隔的特征列表
    rosetta_relax: str = "false"

    def run(self):
        feats_list = [f.strip() for f in self.emp_feats.split(",") if f.strip()]
        do_relax = self.rosetta_relax.lower() == "true"
        return empirical_feats.run(
            input_dir=self.input_dir,
            output_dir=self.output_dir,
            emp_feats=feats_list,
            rosetta_relax=do_relax,
        )


@dataclass
class LMEmbed(BaseParams):
    model_type: str = "esm2"  # esm2, saprot, or both

    def run(self):
        esm2_error = {}
        saprot_error = {}
        all_error = {}
        if self.model_type in ["esm2", "both"]:
            esm2_error = esm2.run(
                input_dir=self.input_dir,
                output_dir=self.output_dir,
                path=path,
            )
        if self.model_type in ["saprot", "both"]:
            saprot_error = saprot.run(
                input_dir=self.input_dir,
                output_dir=self.output_dir,
                path=path,
            )
        else:
            return {"model_type": f"Unknown model type: {self.model_type}"}

        all_error.update(esm2_error)
        for k, v in saprot_error.items():
            if k in all_error:
                all_error[k] += "; " + v
            else:
                all_error[k] = v

        return all_error


class_map = {
    "pred_bs": PredBS,
    "emp_feats": EmpFeats,
    "lm_embed": LMEmbed,
}


def main(params):
    task_name = params["task_name"]
    output_dir = params["output_dir"]
    error = {}

    if params["input_method"] == "id":
        ids_list = [i.strip() for i in params["ids"].split(",") if i.strip()]
        _, download_error = download_pdb(ids_list, params["input_dir"])
        error.update(download_error)

    print(f"[PSKit] Running task: {task_name}")
    print(f"[PSKit] Params: {params}")

    try:
        if task_name not in class_map:
            raise ValueError(f"Unknown task name: {task_name}")

        task_error = class_map[task_name](**params).run()
        error.update(task_error)
    except Exception as e:
        error["__main__"] = str(e) + "\n" + traceback.format_exc()

    if error:
        print(f"[PSKit] Errors: {error}")
        error_file = os.path.join(output_dir, "error.json")
        with open(error_file, "w") as f:
            json.dump(error, f, indent=4)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python -m ai.run_pskit <params.json>")
        sys.exit(1)

    params_file = sys.argv[1]
    if not os.path.exists(params_file):
        print(f"Error: params file not found: {params_file}")
        sys.exit(1)

    with open(params_file, "r") as f:
        params = json.load(f)

    main(params)
