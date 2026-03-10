export function pdbIdFromSource(source, stripExtension, isValidPdbId) {
    const base = stripExtension(String(source || ""))
        .trim()
        .toLowerCase();
    return isValidPdbId(base) ? base : "";
}

export function molstarFormatFromFileName(name, getFormatFromFileName) {
    const ext = getFormatFromFileName(String(name || ""));
    if (ext === "cif") return "mmcif";
    return "pdb";
}

export function tokenizeCifLine(line) {
    return (line.match(/'[^']*'|"[^"]*"|\S+/g) || []).map((t) => t.replace(/^['"]|['"]$/g, ""));
}

export function inferChainSelectorFromStructureText(text, format) {
    const fmt = String(format || "").toLowerCase();
    if (fmt === "pdb") {
        const ids = new Set();
        const lines = String(text || "").split(/\r?\n/);
        for (const line of lines) {
            if (!line || line.length < 22) continue;
            if (!(line.startsWith("ATOM") || line.startsWith("HETATM"))) continue;
            const c = String(line[21] || "").trim();
            if (c) ids.add(c);
        }
        return { field: "auth_asym_id", ids: Array.from(ids) };
    }

    const lines = String(text || "").split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        if (lines[i].trim() !== "loop_") continue;

        const headers = [];
        let j = i + 1;
        for (; j < lines.length; j++) {
            const s = lines[j].trim();
            if (!s) continue;
            if (!s.startsWith("_")) break;
            if (s.startsWith("_atom_site.")) headers.push(s);
        }

        if (headers.length === 0) continue;
        const authIdx = headers.findIndex((h) => h === "_atom_site.auth_asym_id");
        const labelIdx = headers.findIndex((h) => h === "_atom_site.label_asym_id");
        const colIdx = authIdx >= 0 ? authIdx : labelIdx;
        if (colIdx < 0) continue;

        const field = authIdx >= 0 ? "auth_asym_id" : "struct_asym_id";
        const ids = new Set();
        for (; j < lines.length; j++) {
            const s = lines[j].trim();
            if (!s) continue;
            if (s === "#" || s === "loop_" || s.startsWith("_")) break;
            const tokens = tokenizeCifLine(s);
            if (tokens.length <= colIdx) continue;
            const v = String(tokens[colIdx] || "").trim();
            if (v && v !== "." && v !== "?") ids.add(v);
        }

        return { field, ids: Array.from(ids) };
    }

    return { field: "auth_asym_id", ids: [] };
}
