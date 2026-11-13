import type { Story } from "@/lib/site";

export function StoryCard({ story }: { story: Story }) {
  return (
    <figure className="flex h-full flex-col justify-between rounded-card border border-rule bg-paper p-8">
      <blockquote className="text-title font-medium text-balance">
        <p>“{story.quote}”</p>
      </blockquote>
      <figcaption className="mt-8 border-t border-rule pt-4 text-sm">
        <span className="block font-semibold text-ink">{story.name}</span>
        <span className="text-muted">Beta user, moved from {story.from}</span>
      </figcaption>
    </figure>
  );
}
