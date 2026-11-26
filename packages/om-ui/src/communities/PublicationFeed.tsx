// SPDX-License-Identifier: MPL-2.0
import { useId } from "react";
import { RichText } from "../conversations/richtext";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import { CommunityAction, type ActionProps } from "./CommunityAction";
import type { CommunitiesHref, PublicationDetails } from "./communities";

export function PublicationFeed({ publication, copy, unavailable, onChange, hrefFor }: ActionProps & {
  readonly publication: PublicationDetails;
  readonly hrefFor: CommunitiesHref;
}) {
  const followId = useId();
  const t = copy.publication;
  return (
    <>
      <p>{t.publicFeed}</p>
      {publication.description === undefined ? null : <p dir="auto">{publication.description}</p>}
      {publication.provenance === "confirmed" ? (
        <p>{t.provenance.confirmed}</p>
      ) : (
        <SecurityNoticeBanner reasonCode={publication.provenance} label={t.provenance[publication.provenance]} />
      )}
      <p>{publication.following ? t.following : t.notFollowing}</p>
      <p id={followId}>{t.followBoundary}</p>
      {publication.followAction === null ? null : (
        <CommunityAction
          action={publication.followAction} copy={copy} consequenceId={followId}
          unavailable={unavailable} onChange={onChange}
        />
      )}
      {publication.posts.length === 0 ? <p>{t.noPosts}</p> : (
        <ol className="om-community-list om-community-feed" role="list">
          {publication.posts.map((post) => (
            <li className="om-community-stack om-community-post" key={post.id}>
              <p>{copy.formatTime(post.publishedAtMs)}</p>
              {post.state === "retracted" ? (
                <>
                  <p><strong>{t.retracted}</strong></p>
                  {post.reason === undefined ? null : <p dir="auto">{post.reason}</p>}
                  <p>{t.copiesRemain}</p>
                </>
              ) : (
                <>
                  {post.state === "edited" ? <p>{t.edited}</p> : null}
                  <div dir="auto"><RichText doc={post.content} copy={t.richText} /></div>
                </>
              )}
            </li>
          ))}
        </ol>
      )}
      {publication.discussion === undefined ? null : (
        <section className="om-community-stack">
          <h3>{t.discussion}</h3>
          <p>{t.discussionBoundary}</p>
          <div>
            <a className="om-community-link" href={hrefFor({ kind: "room", id: publication.discussion.id })}>
              <bdi>{publication.discussion.name}</bdi>
            </a>
          </div>
        </section>
      )}
    </>
  );
}
