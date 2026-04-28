import Image from "next/image";
import Link from "next/link";

const products = [
  {
    id: 1,
    title: "Handmade Ceramic Vase",
    artisan: "Nina Ceramics",
    price: "12 USDC",
    image: "/bg.jpg",
    category: "Sculpting",
  },
  {
    id: 2,
    title: "Acrylic Landscape Painting",
    artisan: "Paintyourlife",
    price: "25 USDC",
    image: "/bg2.jpg",
    category: "Painting",
  },
  {
    id: 3,
    title: "Crocheted Wall Hanging",
    artisan: "Korve Studio",
    price: "18 USDC",
    image: "/bg3.jpg",
    category: "Crocheting",
  },
  {
    id: 4,
    title: "Hand-Sewn Linen Tote",
    artisan: "Stitch & Co.",
    price: "15 USDC",
    image: "/bg.jpg",
    category: "Sewing",
  },
];

export default function CategoriesResult() {
  return (
    <section className="py-12 px-4 sm:px-6 lg:px-10 max-w-7xl mx-auto">
      <div className="flex items-center justify-between mb-8">
        <h2 className="font-ibm-plex-serif text-3xl sm:text-4xl font-bold text-[#272727]">
          Featured Artisan Products
        </h2>
        <Link
          href="/market"
          className="text-sm font-medium text-[#517A77] hover:underline hidden sm:block"
        >
          View all →
        </Link>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
        {products.map((product) => (
          <Link
            key={product.id}
            href="/market"
            className="group rounded-2xl overflow-hidden bg-white shadow-sm hover:shadow-md transition-shadow duration-200 border border-gray-100"
          >
            <div className="relative h-48 w-full overflow-hidden">
              <Image
                src={product.image}
                alt={product.title}
                fill
                className="object-cover group-hover:scale-105 transition-transform duration-300"
              />
              <span className="absolute top-3 left-3 bg-white/90 text-[#517A77] text-xs font-semibold px-2.5 py-1 rounded-full">
                {product.category}
              </span>
            </div>
            <div className="p-4">
              <h3 className="font-semibold text-[#272727] text-sm leading-snug mb-1 line-clamp-2">
                {product.title}
              </h3>
              <p className="text-xs text-gray-500 mb-3">by {product.artisan}</p>
              <div className="flex items-center justify-between">
                <span className="text-[#C4928F] font-bold text-sm">{product.price}</span>
                <span className="text-xs text-white bg-[#517A77] px-3 py-1 rounded-lg">
                  Buy Now
                </span>
              </div>
            </div>
          </Link>
        ))}
      </div>

      <div className="flex justify-center mt-6 sm:hidden">
        <Link href="/market" className="text-sm font-medium text-[#517A77] hover:underline">
          View all products →
        </Link>
      </div>
    </section>
  );
}
