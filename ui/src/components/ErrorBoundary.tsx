import { Component, type ErrorInfo, type ReactNode } from "react";
import { noteError } from "../lib/problems";

/**
 * Catches a crash so it stays inside the panel it happened in.
 *
 * Without one of these a single thrown error unmounts the whole tree and
 * leaves a black window — which is what a tester saw when they picked a log
 * from a combined upload, and all the report could say was "it goes black".
 * A crash in one panel is now a message in that panel, with the rest of the
 * page still usable and the error written down where it can be copied.
 */
interface Props {
  /** What failed, for the message and the report: "the scoreboard". */
  what: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
  stack: string | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, stack: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ stack: info.componentStack ?? null });
    noteError({
      what: this.props.what,
      message: error.message || String(error),
      detail: [error.stack, info.componentStack].filter(Boolean).join("\n\n"),
    });
  }

  render() {
    const { error, stack } = this.state;
    if (!error) return this.props.children;
    return (
      <section className="panel crash">
        <h2>{this.props.what} stopped working</h2>
        <p className="hint">
          The rest of the page still works. Settings › Problems has this written down, with a button to copy it.
        </p>
        <pre className="crash-msg">{error.message || String(error)}</pre>
        <div className="row">
          <button onClick={() => this.setState({ error: null, stack: null })}>Try again</button>
          <button
            className="linkish"
            onClick={() => {
              const text = [`${this.props.what}: ${error.message}`, error.stack, stack]
                .filter(Boolean)
                .join("\n\n");
              void navigator.clipboard?.writeText(text);
            }}
          >
            Copy the details
          </button>
        </div>
      </section>
    );
  }
}
