import os
import subprocess
from pathlib import Path
import shutil
import traceback
from ..config import path


def run_dssp(input_dir, output_dir):
    error = {}
    pdb_files = [os.path.join(input_dir, f) for f in os.listdir(input_dir) if f.endswith(".pdb") or f.endswith(".cif")]
    for pdb_file in pdb_files:
        try:
            prot_name = os.path.splitext(os.path.basename(pdb_file))[0]
            dssp_file = os.path.join(output_dir, prot_name + ".dssp")
            if not os.path.isfile(dssp_file):
                cmd = [path["dssp"], "--output-format", "dssp", pdb_file, dssp_file]
                result = subprocess.run(cmd, capture_output=True, text=True)
                if result.returncode != 0:
                    raise Exception(f"DSSP failed: {result.stderr}")
        except Exception as e:
            error[prot_name] = str(e)
    return error


def _find_rosetta_exe(candidates: list[str]) -> str:
    for name in candidates:
        p = shutil.which(name)
        if p:
            return p
    raise FileNotFoundError("Can't find Rosetta executable among candidates: " + ", ".join(candidates))


def _cif_to_pdb_if_possible(cif_path: Path) -> Path | None:
    out_pdb = cif_path.with_suffix(".pdb")
    if out_pdb.exists():
        return out_pdb

    try:
        from Bio.PDB import MMCIFParser, PDBIO  # type: ignore

        parser = MMCIFParser(QUIET=True)
        structure = parser.get_structure(cif_path.stem, str(cif_path))
        io = PDBIO()
        io.set_structure(structure)
        io.save(str(out_pdb))
        return out_pdb
    except Exception:
        return None


def run_rosetta_score(input_dir, output_dir, do_relax):
    error = {}

    input_dir = Path(input_dir)
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    relax_exe = None
    if do_relax:
        relax_exe = _find_rosetta_exe(
            [
                "relax.linuxgccrelease",
                "relax.default.linuxgccrelease",
                "relax",
            ]
        )
    score_exe = _find_rosetta_exe(
        [
            "score_jd2.linuxgccrelease",
            "score_jd2.default.linuxgccrelease",
            "score_jd2",
        ]
    )

    exts = {".pdb", ".cif"}
    inputs = sorted([p for p in input_dir.iterdir() if p.is_file() and p.suffix.lower() in exts])
    if not inputs:
        raise ValueError(f"Can't find any PDB/CIF files")

    for in_path in inputs:
        try:
            stem = in_path.stem
            # 1.cif转pdb
            structure_to_score = in_path
            if in_path.suffix.lower() in {".cif"}:
                converted = _cif_to_pdb_if_possible(in_path)
                if converted is not None:
                    structure_to_score = converted
                else:
                    structure_to_score = in_path  # Use original CIF if conversion fails
            # 2.rosetta relax
            if do_relax:
                relax_cmd = [
                    relax_exe,
                    "-in:file:s",
                    str(structure_to_score),
                    "-nstruct",
                    "1",
                    "-relax:fast",
                    "-score:weights",
                    "ref2015",
                    "-out:path:all",
                    str(output_dir),
                    "-out:suffix",
                    "_relaxed",
                    "-overwrite",
                ]

                result = subprocess.run(relax_cmd, capture_output=True, text=True)
                if result.returncode != 0:
                    raise Exception(f"Rosetta relax failed: {result.stderr}")

                relaxed_pdb = os.path.join(output_dir, f"{stem}_relaxed_0001.pdb")

                if not os.path.exists(relaxed_pdb):
                    raise Exception(f"Rosetta relax failed")

                structure_to_score = relaxed_pdb
            # 3.rosetta score
            scorefile = os.path.join(output_dir, f"{stem}_score.txt")
            score_cmd = [
                score_exe,
                "-in:file:s",
                str(structure_to_score),
                "-score:weights",
                "ref2015",
                "-out:file:scorefile",
                str(scorefile),
                "-overwrite",
            ]
            result = subprocess.run(score_cmd, capture_output=True, text=True, check=True)
            if result.returncode != 0:
                raise Exception(f"Rosetta score failed: {result.stderr}")
        except Exception as e:
            error[in_path.name] = str(e)

    return error


def run(input_dir, output_dir, emp_feats, rosetta_relax):
    pdb_files = [os.path.join(input_dir, f) for f in os.listdir(input_dir) if f.endswith(".pdb") or f.endswith(".cif") or f.endswith(".mmcif")]
    os.makedirs(output_dir, exist_ok=True)

    error = {}

    if "dssp" in emp_feats:
        try:
            dssp_error = run_dssp(input_dir, output_dir)
            error.update(dssp_error)
        except Exception as e:
            error["run_DSSP"] = str(e) + "\n" + traceback.format_exc()

    if "rosetta" in emp_feats:
        try:
            rosetta_error = run_rosetta_score(input_dir, output_dir, rosetta_relax)
            error.update(rosetta_error)
        except Exception as e:
            error["run_Rosetta"] = str(e) + "\n" + traceback.format_exc()

    return error


if __name__ == "__main__":
    input_dir = "/home/zh/test/input"
    output_dir = "/home/zh/test/output"
    emp_feats = ["dssp", "rosetta"]  # Specify which empirical features to compute
    run(input_dir, output_dir, emp_feats, rosetta_relax=False)
