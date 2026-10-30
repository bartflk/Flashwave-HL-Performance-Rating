import { friendlyError } from "../lib/apierror";

/** An error in a line people can read; the full text on hover. */
export function ApiError({ error, className = "error" }: { error: unknown; className?: string }) {
  const { text, detail } = friendlyError(error);
  return (
    <p className={className} title={text === detail ? undefined : detail}>
      {text}
    </p>
  );
}
