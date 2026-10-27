import { mount, Storage } from "nsh";
import { createApp } from "../../app/application";
import { observations } from "./support/observations";

// Isolated public connection port. Production never reads these fixture keys.
void (async () => {
  const fixture = JSON.parse((await Storage.getItem("test.mobileApp"))!);
  const projection = fixture.projection;
  const commands: unknown[] = [];
  mount(createApp({
    ...observations(),
    foreground: () => true,
    remoteNavigation: () => fixture.formFactor === "tv",
    link: {
      initialize: () => JSON.stringify(projection),
      poll: () => JSON.stringify(projection),
      command: (json) => {
        const command = JSON.parse(json);
        commands.push(command);
        if (command.op === "rename") projection.deviceName = command.name;
        void Storage.setItem("test.connectionCommands", JSON.stringify(commands));
      },
    },
  }));
})();
