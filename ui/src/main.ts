import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import "./theme-finance.css";
import "./theme-trading.css";
// After the styles: it adds depth to their rules rather than fighting them.
import "./theme-depth.css";
import { applyTheme } from "./theme.svelte";

// Before the first paint, so a light theme never flashes dark.
applyTheme();

export default mount(App, { target: document.getElementById("app")! });
