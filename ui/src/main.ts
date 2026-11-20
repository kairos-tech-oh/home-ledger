import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import "./theme-finance.css";
import "./theme-trading.css";
import { applyTheme } from "./theme.svelte";

// Before the first paint, so a light theme never flashes dark.
applyTheme();

export default mount(App, { target: document.getElementById("app")! });
