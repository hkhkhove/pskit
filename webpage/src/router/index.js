import { createRouter, createWebHistory } from "vue-router";
import Home from "../views/Home.vue";
import BindingAnnotate from "../views/binding/Annotate.vue";
import BindingPredict from "../views/binding/Predict.vue";
import FeaturesStructural from "../views/features/Structural.vue";
import FeaturesLanguageModel from "../views/features/LanguageModel.vue";
import ToolsSplit from "../views/tools/Split.vue";
import ToolsExtract from "../views/tools/Extract.vue";
import ContactMap from "../views/ContactMap.vue";
import Viewer from "../views/Viewer.vue";
import NotFound from "../views/NotFound.vue";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: "/",
      name: "Home",
      component: Home,
    },
    {
      path: "/binding/annotate",
      name: "BindingAnnotate",
      component: BindingAnnotate,
    },
    {
      path: "/binding/predict",
      name: "BindingPredict",
      component: BindingPredict,
    },
    {
      path: "/features/structural",
      name: "FeaturesStructural",
      component: FeaturesStructural,
    },
    {
      path: "/features/language-model",
      name: "FeaturesLanguageModel",
      component: FeaturesLanguageModel,
    },
    {
      path: "/tools/split",
      name: "ToolsSplit",
      component: ToolsSplit,
    },
    {
      path: "/tools/extract",
      name: "ToolsExtract",
      component: ToolsExtract,
    },
    {
      path: "/contact-map",
      name: "ContactMap",
      component: ContactMap,
    },
    {
      path: "/viewer",
      name: "Viewer",
      component: Viewer,
    },
    {
      path: "/:pathMatch(.*)*",
      name: "NotFound",
      component: NotFound,
    },
  ],
});

export default router;
