import { t, type Language } from "../i18n";
import { formatTimecode, isMusicMarker, selectedIndex, type EditorLine } from "./model";
import { markerLabelFits, xAtTime, type Tick } from "./timeline";

export type LineHandlers = {
  select: (id: string) => void;
  seek: (id: string) => void;
  commitTime: (id: string, value: string) => void;
  setText: (id: string, value: string) => void;
  remove: (id: string) => void;
  editEnd: (id: string) => void;
};

export function renderLines(
  list: HTMLElement,
  lines: EditorLine[],
  selectedId: string | null,
  language: Language,
  handlers: LineHandlers,
): void {
  const rows = lines.map((line, index) => {
    const row = document.createElement("li");
    row.className = "line-row";
    row.dataset.id = line.id;
    if (line.id === selectedId) {
      row.classList.add("selected");
    }
    if (isMusicMarker(line.text)) {
      row.classList.add("music");
    }
    row.addEventListener("click", () => handlers.select(line.id));
    row.addEventListener("focusin", () => handlers.select(line.id));

    const number = document.createElement("span");
    number.className = "line-number";
    number.textContent = String(index + 1);

    const time = document.createElement("input");
    time.className = "line-time";
    time.type = "text";
    time.inputMode = "decimal";
    time.spellcheck = false;
    time.value = formatTimecode(line.t);
    time.setAttribute("aria-label", t(language, "studio.line_time", { index: index + 1 }));
    time.addEventListener("blur", () => handlers.editEnd(line.id));
    time.addEventListener("change", () => handlers.commitTime(line.id, time.value));

    const text = document.createElement("input");
    text.className = "line-text";
    text.type = "text";
    text.spellcheck = false;
    text.value = line.text;
    text.placeholder = t(language, "studio.text_placeholder");
    text.setAttribute("aria-label", t(language, "studio.line_text", { index: index + 1 }));
    text.addEventListener("blur", () => handlers.editEnd(line.id));
    text.addEventListener("input", () => handlers.setText(line.id, text.value));

    const goTo = iconButton("→", t(language, "studio.line_go", { index: index + 1 }));
    goTo.addEventListener("click", (event) => {
      event.stopPropagation();
      handlers.seek(line.id);
    });

    const remove = iconButton("✕", t(language, "studio.line_delete", { index: index + 1 }));
    remove.classList.add("danger");
    remove.addEventListener("click", (event) => {
      event.stopPropagation();
      handlers.remove(line.id);
    });

    row.append(number, time, text, goTo, remove);
    return row;
  });

  list.replaceChildren(...rows);
}

export function updateLineRowSelection(list: HTMLElement, selectedId: string | null): void {
  list.querySelectorAll<HTMLElement>(".line-row").forEach((row) => {
    row.classList.toggle("selected", row.dataset.id === selectedId);
  });
}

export function updateLineTimeInput(list: HTMLElement, id: string, time: number | null): void {
  const input = list.querySelector<HTMLInputElement>(`.line-row[data-id="${id}"] .line-time`);
  if (input && document.activeElement !== input) {
    input.value = formatTimecode(time);
  }
}

export function scrollLineIntoView(list: HTMLElement, id: string | null): void {
  if (!id) {
    return;
  }
  const row = list.querySelector<HTMLElement>(`.line-row[data-id="${id}"]`);
  row?.scrollIntoView({ block: "nearest" });
}

export type TimelineElements = {
  scroll: HTMLElement;
  canvas: HTMLElement;
  ruler: HTMLElement;
  markers: HTMLElement;
  playhead: HTMLElement;
};

export type MarkerHandlers = {
  select: (id: string) => void;
  dragStart: (id: string, event: PointerEvent) => void;
};

export function renderTimeline(
  elements: TimelineElements,
  lines: EditorLine[],
  selectedId: string | null,
  scale: number,
  width: number,
  language: Language,
  handlers: MarkerHandlers,
): void {
  elements.canvas.style.width = `${width}px`;
  elements.canvas.style.setProperty("--scale", `${scale}px`);

  const timed = lines
    .flatMap((line) => (line.t === null ? [] : [{ line, x: xAtTime(line.t, scale) }]))
    .sort((left, right) => left.x - right.x);
  const labelFits = markerLabelFits(timed.map((entry) => entry.x));

  const markers = timed.map(({ line, x }, index) => {
    const marker = document.createElement("div");
    marker.className = "marker";
    marker.dataset.id = line.id;
    marker.style.left = `${x}px`;
    marker.title = `${formatTimecode(line.t)} ${line.text}`.trim();
    marker.setAttribute("role", "presentation");
    if (line.id === selectedId) {
      marker.classList.add("selected");
    }
    if (isMusicMarker(line.text)) {
      marker.classList.add("music");
    }
    marker.addEventListener("pointerdown", (event) => handlers.dragStart(line.id, event));

    const showLabels = labelFits[index] || line.id === selectedId;
    const time = document.createElement("span");
    time.className = "marker-time";
    time.textContent = formatTimecode(line.t);
    time.hidden = !showLabels;

    const label = document.createElement("span");
    label.className = "marker-text";
    label.textContent = isMusicMarker(line.text) ? t(language, "studio.music") : line.text;
    label.hidden = !showLabels;

    marker.append(time, label);
    return marker;
  });

  elements.markers.replaceChildren(...markers);
}

// The timeline's label renderer takes the active language from the controller.
export function renderRuler(elements: TimelineElements, ticks: Tick[], scale: number): void {
  const labels = ticks.map((tick) => {
    const marker = document.createElement("span");
    marker.className = tick.major ? "tick major" : "tick";
    marker.style.left = `${xAtTime(tick.time, scale)}px`;
    if (tick.major) {
      const label = document.createElement("span");
      label.className = "tick-label";
      label.textContent = tick.label;
      marker.append(label);
    }
    return marker;
  });
  elements.ruler.replaceChildren(...labels);
}

export function updatePlayhead(
  playhead: HTMLElement,
  position: number | null,
  scale: number,
): void {
  if (position === null) {
    playhead.hidden = true;
    return;
  }
  playhead.hidden = false;
  playhead.style.transform = `translateX(${xAtTime(position, scale)}px)`;
}

export function updateMarkerTime(
  markers: HTMLElement,
  id: string,
  time: number,
  scale: number,
): void {
  const marker = markers.querySelector<HTMLElement>(`.marker[data-id="${id}"]`);
  if (!marker) {
    return;
  }
  marker.style.left = `${xAtTime(time, scale)}px`;
  const label = marker.querySelector<HTMLElement>(".marker-time");
  if (label) {
    label.textContent = formatTimecode(time);
  }
  marker.title =
    `${formatTimecode(time)} ${marker.querySelector(".marker-text")?.textContent ?? ""}`.trim();
}

export function updateMarkerSelection(markers: HTMLElement, selectedId: string | null): void {
  markers.querySelectorAll<HTMLElement>(".marker").forEach((marker) => {
    marker.classList.toggle("selected", marker.dataset.id === selectedId);
  });
}

export type InspectorState = {
  line: EditorLine | null;
  lines: EditorLine[];
  language: Language;
  canTakeTime: boolean;
  canPreview: boolean;
  previewing: boolean;
};

export type InspectorHandlers = {
  nudge: (delta: number) => void;
  takeTime: () => void;
  preview: () => void;
  toggleMusic: () => void;
  remove: () => void;
};

const NUDGES = [-0.5, -0.1, -0.01, 0.01, 0.1, 0.5];

export function renderInspector(
  body: HTMLElement,
  state: InspectorState,
  handlers: InspectorHandlers,
): void {
  const children: HTMLElement[] = [];
  const line = state.line;

  if (!line) {
    const hint = document.createElement("p");
    hint.className = "hint";
    hint.textContent = t(state.language, "studio.inspector.empty");
    children.push(hint);
  } else {
    const index = selectedIndex(state.lines, line.id);

    const head = document.createElement("div");
    head.className = "inspector-head";
    const number = document.createElement("span");
    number.className = "inspector-index";
    number.textContent = `#${index + 1}`;
    const time = document.createElement("span");
    time.className = "inspector-time";
    time.id = "inspector-time";
    time.textContent = line.t === null ? "—" : formatTimecode(line.t);
    head.append(number, time);

    const preview = document.createElement("p");
    preview.className = "inspector-text";
    preview.textContent = isMusicMarker(line.text)
      ? t(state.language, "studio.music")
      : line.text.trim()
        ? line.text
        : t(state.language, "studio.text_empty");
    children.push(head, preview);

    const nudgeLabel = document.createElement("p");
    nudgeLabel.className = "inspector-label";
    nudgeLabel.textContent = t(state.language, "studio.inspector.nudge");
    children.push(nudgeLabel);

    const nudges = document.createElement("div");
    nudges.className = "inspector-nudges";
    for (const delta of NUDGES) {
      const button = ghostButton(formatDelta(delta));
      button.disabled = line.t === null;
      button.title = t(state.language, "studio.inspector.nudge_by", {
        value: formatDelta(delta),
      });
      button.addEventListener("click", () => handlers.nudge(delta));
      nudges.append(button);
    }
    children.push(nudges);

    const actions = document.createElement("div");
    actions.className = "inspector-actions";

    const take = ghostButton(t(state.language, "studio.inspector.take_time"));
    take.disabled = !state.canTakeTime;
    take.addEventListener("click", handlers.takeTime);
    actions.append(take);

    const previewButton = ghostButton(
      t(
        state.language,
        state.previewing ? "studio.inspector.previewing" : "studio.inspector.preview",
      ),
    );
    previewButton.disabled = !state.canPreview || line.t === null;
    previewButton.addEventListener("click", handlers.preview);
    actions.append(previewButton);

    const music = ghostButton(
      t(
        state.language,
        isMusicMarker(line.text) ? "studio.inspector.music_off" : "studio.inspector.music_on",
      ),
    );
    music.addEventListener("click", handlers.toggleMusic);
    actions.append(music);

    const remove = ghostButton(t(state.language, "studio.inspector.delete"));
    remove.classList.add("danger");
    remove.addEventListener("click", handlers.remove);
    actions.append(remove);

    children.push(actions);
  }

  body.replaceChildren(...children);
}

export function updateInspectorTime(body: HTMLElement, time: number | null): void {
  const target = body.querySelector<HTMLElement>("#inspector-time");
  if (target) {
    target.textContent = time === null ? "—" : formatTimecode(time);
  }
}

function formatDelta(delta: number): string {
  return `${delta > 0 ? "+" : ""}${delta}s`;
}

function ghostButton(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost";
  button.textContent = label;
  return button;
}

function iconButton(label: string, ariaLabel: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost icon";
  button.textContent = label;
  button.setAttribute("aria-label", ariaLabel);
  return button;
}
