import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

function handleDragStart(e: React.MouseEvent) {
  // Only drag on left click, not on buttons
  if (e.button === 0) {
    appWindow.startDragging();
  }
}

export default function TitleBar() {
  return (
    <div
      onMouseDown={handleDragStart}
      className="flex items-center justify-between h-9 bg-steam-bg select-none shrink-0"
    >
      {/* App title */}
      <div className="flex items-center gap-2 pl-3 text-xs text-steam-text-dim tracking-wide pointer-events-none">
        <span className="font-semibold text-steam-text">
          Gamekeeper
        </span>
      </div>

      {/* Window controls — pointer-events restored so clicks work */}
      <div className="flex h-full pointer-events-auto">
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => appWindow.minimize()}
          className="w-12 h-full flex items-center justify-center text-steam-text-dim hover:bg-steam-surface-light hover:text-white transition-colors"
          tabIndex={-1}
        >
          <svg width="10" height="1" viewBox="0 0 10 1" fill="currentColor">
            <rect width="10" height="1" />
          </svg>
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => appWindow.toggleMaximize()}
          className="w-12 h-full flex items-center justify-center text-steam-text-dim hover:bg-steam-surface-light hover:text-white transition-colors"
          tabIndex={-1}
        >
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            strokeWidth="1"
          >
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => appWindow.close()}
          className="w-12 h-full flex items-center justify-center text-steam-text-dim hover:bg-red-600 hover:text-white transition-colors"
          tabIndex={-1}
        >
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            stroke="currentColor"
            strokeWidth="1.2"
          >
            <line x1="0" y1="0" x2="10" y2="10" />
            <line x1="10" y1="0" x2="0" y2="10" />
          </svg>
        </button>
      </div>
    </div>
  );
}
