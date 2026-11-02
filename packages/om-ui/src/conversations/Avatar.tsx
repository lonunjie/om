import "./conversations.css";

/** The first letter of a name in a quiet circle: enough to tell chats apart at a glance, with no image to load or leak. */
export const initialOf = (name: string): string => {
  const first = Array.from(name.trim())[0];
  return first === undefined ? "?" : first.toLocaleUpperCase();
};

export const Avatar = ({ name, size = "md" }: { readonly name: string; readonly size?: "md" | "lg" }) => (
  <span className="om-avatar" data-size={size} aria-hidden="true">{initialOf(name)}</span>
);
