import { mount } from "svelte";
import App from "./App.svelte";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./app.css";

const app = mount(App, { target: document.getElementById("app")! });
export default app;
