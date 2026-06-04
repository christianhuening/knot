// Excalidraw ships its stylesheet at `@excalidraw/excalidraw/index.css`,
// but the package's `./*` exports entry only resolves types for `.d.ts`
// files. Declare the side-effect module so TS lets us `import` the CSS.
declare module "@excalidraw/excalidraw/index.css";
