<script setup>
import Card from "../components/Card.vue";

import languageModelImg from "../assets/images/language_model.webp";
import structuralImg from "../assets/images/structural.webp";
import nbspImg from "../assets/images/nbsp.webp";
import nbsaImg from "../assets/images/nbsa.webp";
import pnipImg from "../assets/images/pnip.webp";
import splitImg from "../assets/images/split.webp";
import extractImg from "../assets/images/extract.webp";
import contactMapImg from "../assets/images/contact_map.webp";
import viewerImg from "../assets/images/viewer.webp";

const cards = [
    {
        title: "Annotate Binding Sites",
        description: "Compute protein-nucleic acid binding residue pairs by distance cutoff; export CSV and inspect in 3D.",
        image: nbsaImg,
        router: "/binding/nbsa",
    },
    {
        title: "Predict Binding Sites",
        description: "Predict DNA/RNA-binding residues with scores; visualize predictions in 3D and download CSV.",
        image: nbspImg,
        router: "/binding/nbsp",
    },
    {
        title: "Protein-Nucleic Interaction Prediction",
        description: "Predict whether protein and nucleic-acid sequence pairs interact; submit in batch and download scored CSV.",
        image: pnipImg,
        router: "/binding/pnip",
    },
    {
        title: "Structural Feature Extraction",
        description: "Extract DSSP secondary structure and optional Rosetta energy scores from PDB/CIF structures.",
        image: structuralImg,
        router: "/features/structural",
    },
    {
        title: "Language Model Embedding",
        description: "Generate ESM-2 / SaProt embeddings from sequence or structure for downstream modeling.",
        image: languageModelImg,
        router: "/features/language-model",
    },
    {
        title: "Split Complex",
        description: "Split a complex structure by chain or by molecule type (protein vs nucleic acid) and download parts.",
        image: splitImg,
        router: "/tools/split",
    },
    {
        title: "Extract Fragment",
        description: "Extract a residue range from a selected chain into a new PDB/CIF fragment for analysis.",
        image: extractImg,
        router: "/tools/extract",
    },
    {
        title: "Contact Map",
        description: "Generate a residue distance/contact map matrix and heatmap for a chain or all chains.",
        image: contactMapImg,
        router: "/contact-map",
    },
    {
        title: "Molecule Viewer",
        description: "Open a PDB ID or uploaded structure in Mol* for interactive 3D inspection.",
        image: viewerImg,
        router: "/viewer",
    },
];
</script>

<template>
    <main class="mx-auto max-w-7xl px-4 py-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 justify-items-start gap-x-14 gap-y-8">
            <Card v-for="card in cards" :key="card.title" :title="card.title" :description="card.description"
                :image="card.image" :router="card.router" />
        </div>
    </main>
</template>
