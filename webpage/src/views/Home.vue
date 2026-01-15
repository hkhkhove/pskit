<script setup>
import Card from "../components/Card.vue";

// 导入插图
import languageModelImg from "../../assets/images/language_model.png";
import structuralImg from "../../assets/images/structural.png";
import predictImg from "../../assets/images/predict.png";
import annotateImg from "../../assets/images/annotate.png";
import splitImg from "../../assets/images/split.png";
import extractImg from "../../assets/images/extract.jpeg";
import contactMapImg from "../../assets/images/contact_map.jpeg";
import viewerImg from "../../assets/images/viewer.jpeg";
</script>

<template>
    <main>
        <div class="flex flex-wrap justify-evenly max-w-7xl min-w-sm mx-auto my-4">
            <Card title="Annotate Binding Sites" :image="annotateImg" router="/binding/annotate" />
            <Card title="Predict Binding Sites" :image="predictImg" router="/binding/predict" />
            <Card title="Structural Feature Extraction" :image="structuralImg" router="/features/structural" />
            <Card title="Language Model Embedding" :image="languageModelImg" router="/features/language-model" />
            <Card title="Split Complex" :image="splitImg" router="/tools/split" />
            <Card title="Extract Fragment" :image="extractImg" router="/tools/extract" />
            <Card title="Contact Map" :image="contactMapImg" router="/contact-map" />
            <Card title="Molecule Viewer" :image="viewerImg" router="/viewer" />
        </div>
    </main>
</template>
