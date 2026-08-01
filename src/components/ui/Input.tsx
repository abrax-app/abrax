import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

// `forwardRef` para que quien abra un diálogo pueda poner el foco AQUÍ al
// entrar. Sin esto, en React 18 la prop `ref` sobre un componente de función se
// descarta en silencio: no falla, simplemente el foco nunca llega.
export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  function Input(
    { className = "", variant = "default", disabled, ...props },
    ref,
  ) {
    const baseClasses =
      "px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded-md text-start transition-all duration-150";

    const interactiveClasses = disabled
      ? "opacity-60 cursor-not-allowed bg-mid-gray/10 border-mid-gray/40"
      : "hover:bg-logo-primary/10 hover:border-logo-primary focus:outline-none focus:bg-logo-primary/20 focus:border-logo-primary";

    const variantClasses = {
      default: "px-3 py-2",
      compact: "px-2 py-1",
    } as const;

    return (
      <input
        ref={ref}
        className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
        disabled={disabled}
        {...props}
      />
    );
  },
);
