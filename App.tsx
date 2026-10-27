import { mount } from "nsh";
import { Link, UiLifecycle } from "loki-metris-mobile-display/native";
import { createApp } from "./src/app/application";

mount(createApp({ link: Link, foreground: () => UiLifecycle.foreground(), remoteNavigation: () => UiLifecycle.formFactor() === "tv" }));
