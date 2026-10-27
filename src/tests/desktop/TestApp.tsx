import { mount } from "nsh";
import { UiLifecycle } from "loki-metris-mobile-display/native";
import { createApp } from "../../app/application";
import { FakeLink } from "./support/fake-link";
import { observations } from "./support/observations";

void FakeLink.load().then((link) => {
  mount(createApp({ link, foreground: () => UiLifecycle.foreground(), ...observations() }));
});
