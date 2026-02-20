"use client";

import Image from "next/image";
import { useState } from "react";

interface TestimonialAvatarProps {
  src: string;
  name: string;
}

export function TestimonialAvatar({ src, name }: TestimonialAvatarProps) {
  const [hasError, setHasError] = useState(false);
  const initial = name.charAt(0).toUpperCase();

  return (
    <div
      className={[
        "absolute left-1/2 -translate-x-1/2 -top-12 ",
        "w-24 h-24 ",
        "shadow-[0_20px_20px_rgba(100,40,40,0.22)] rounded-full overflow-hidden ",
        "flex items-center justify-center",
      ].join(" ")}
    >
      {!hasError ? (
        <Image
          src={src}
          alt={name}
          fill
          sizes="96px"
          className="object-cover "
          onError={() => setHasError(true)}
        />
      ) : (
        <span
          aria-hidden="true"
          className="text-white text-3xl font-bold font-serif select-none"
        >
          {initial}
        </span>
      )}
    </div>
  );
}
