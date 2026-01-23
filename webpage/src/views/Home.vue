<script setup>
import Card from "../components/Card.vue";

import languageModelImg from "../assets/images/language_model.webp";
import structuralImg from "../assets/images/structural.webp";
import predictImg from "../assets/images/predict.webp";
import annotateImg from "../assets/images/annotate.webp";
import splitImg from "../assets/images/split.webp";
import extractImg from "../assets/images/extract.webp";
import contactMapImg from "../assets/images/contact_map.webp";
import viewerImg from "../assets/images/viewer.webp";
</script>

<template>
    <main class="mx-auto max-w-7xl px-4 py-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 justify-items-start gap-x-14 gap-y-8">
            <Card title="Annotate Binding Sites" description="Compute protein-nucleic acid binding residue pairs by distance cutoff; export CSV and inspect in 3D." :image="annotateImg" router="/binding/annotate" />
            <Card title="Predict Binding Sites" description="Predict DNA/RNA-binding residues with scores; visualize predictions in 3D and download CSV." :image="predictImg" router="/binding/predict" />
            <Card title="Structural Feature Extraction" description="Extract DSSP secondary structure and optional Rosetta energy scores from PDB/CIF structures." :image="structuralImg" router="/features/structural" />
            <Card title="Language Model Embedding" description="Generate ESM-2 / SaProt embeddings from sequence or structure for downstream modeling." :image="languageModelImg" router="/features/language-model" />
            <Card title="Split Complex" description="Split a complex structure by chain or by molecule type (protein vs nucleic acid) and download parts." :image="splitImg" router="/tools/split" />
            <Card title="Extract Fragment" description="Extract a residue range from a selected chain into a new PDB/CIF fragment for analysis." :image="extractImg" router="/tools/extract" />
            <Card title="Contact Map" description="Generate a residue distance/contact map matrix and heatmap for a chain or all chains." :image="contactMapImg" router="/contact-map" />
            <Card title="Molecule Viewer" description="Open a PDB ID or uploaded structure in Mol* for interactive 3D inspection." :image="viewerImg" router="/viewer" />
        </div>
    </main>
</template>
