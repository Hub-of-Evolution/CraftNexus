// Purely presentational — no client directive needed.

interface StarRatingProps {
  rating: number;
  max?: number;
}

export function StarRating({ rating, max = 5 }: StarRatingProps) {
  return (
    <div
      className="flex items-center gap-1"
      role="img"
      aria-label={`${rating} out of ${max} stars`}
    >
      {Array.from({ length: max }, (_, i) => (
        <svg
          key={i}
          className="w-[18px] h-[18px] shrink-0"
          viewBox="0 0 20 20"
          aria-hidden="true"
        >
          <path
            d="M10 1l2.39 4.84 5.34.78-3.86 3.76.91 5.31L10 13.27l-4.78 2.52.91-5.31L2.27 6.62l5.34-.78z"
            fill={i < rating ? "#C4928F" : "#ddc4c4"}
          />
        </svg>
      ))}
    </div>
  );
}
