import { mount } from "svelte";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
// Cormorant Garamond italic — used by the Smriti wordmark logo
// (`assets/smriti-logo.svg`) and unlocks display-italic moments
// like PhotoDetail's "when" date and PageHero. Italic 500 only,
// ~30 KB, latin subset.
import "@fontsource/cormorant-garamond/500-italic.css";
import App from "./App.svelte";
import "./app.css";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");

const app = mount(App, { target });
export default app;
