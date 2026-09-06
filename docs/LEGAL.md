# The legal posture, and why it is shaped this way

> **Not legal advice.** This is an engineering document recording the reasoning
> behind decisions already made. Before the first public release, have a
> licensed attorney in the formation state review [`TERMS.md`](../TERMS.md) and
> [`PRIVACY.md`](../PRIVACY.md). Budget one to three hours of their time; this
> is a well-trodden document set and should not be expensive.

Swept destroys data on purpose. That single fact drives everything below.

## The five layers

No one of these is the defence. They work because they overlap: a plaintiff has
to get past all five, and each one fails differently.

| # | Layer | Where it lives | What it does |
|---|---|---|---|
| 1 | **Design** — preview, consent, Trash-not-unlink, audit log | `crates/safety`, `crates/core` | Prevents the harm, and reduces damages when harm occurs |
| 2 | **Licence disclaimer** | [`LICENSE`](../LICENSE) | Disclaims warranty and liability for the source |
| 3 | **Supplemental terms** | [`TERMS.md`](../TERMS.md) | Enumerates the specific damages disclaimed, adds a cap |
| 4 | **Assent** | First-run acknowledgement in the GUI and CLI | Turns 2 and 3 from *published* into *agreed* |
| 5 | **Entity** | The LLC | Separates business liability from personal assets |

**Layer 1 is the strongest and it is made of code.** Every safety property in
the SAFETY CONTRACT is also a legal asset: the preview means the user saw what
would happen, the per-consequence acknowledgements mean they were told what it
would cost, and the audit log is a contemporaneous business record of exactly
what was shown and what was confirmed. If there is ever a dispute about whether
someone was warned, `audit.jsonl` is the answer.

This is worth stating plainly because it inverts the usual instinct: **the
disclaimers are the weaker half of this.** Do not let anyone weaken the
consent design on the grounds that the terms cover it.

## Why MIT stayed, and how it was made equivalent

Apache-2.0 was considered and rejected. Its §8 is the better-drafted liability
clause — it names "loss of data" and "computer failure or malfunction"
explicitly, where MIT stops at "ANY CLAIM, DAMAGES OR OTHER LIABILITY". For a
tool whose failure mode is destroyed data, that specificity is exactly the
thing you want.

**But specificity is portable and assent is not.** Four observations decided it:

1. **MIT's disclaimer is already real.** It disclaims warranty in all caps,
   names `MERCHANTABILITY` and `FITNESS FOR A PARTICULAR PURPOSE` by name —
   which is what UCC §2-316 asks for — and its liability clause is broad rather
   than narrow: *any* claim, *any* liability, arising from **or in connection
   with** the software. MIT's weakness is that it is terse, not that it is
   permissive.

2. **The enumeration can be added without touching the licence.** §5 of
   `TERMS.md` does what Apache §8 does and then more: it names data loss first
   and separately, adds "even if advised of the possibility", forecloses the
   *failure of essential purpose* escape hatch, and sets an aggregate cap. None
   of that required changing `LICENSE`.

3. **Assent beats drafting.** A disclaimer in a file nobody opened is
   *browsewrap*, and courts enforce it inconsistently. A disclaimer the user
   was shown and actively agreed to is *clickwrap*, and courts enforce it
   routinely. Swapping MIT for Apache-2.0 would have improved the wording of
   something the user never reads. The first-run acknowledgement improves
   whether any of it binds at all. **That is the larger effect by a wide
   margin**, and it is available under either licence.

4. **MIT is what a small open-source Mac utility is expected to carry.** It is
   read at a glance, needs no compliance work from anyone redistributing it,
   and Homebrew, Nix and every packager already know what to do with it.

**What MIT genuinely lacks and terms cannot supply:** Apache-2.0 §3's express
patent grant, and §5's rule that contributions come in under the same terms.
For a file-scanning utility built on `walkdir` and `plist`, patent exposure is
about as close to zero as software gets — and the contributor question is
handled instead by the DCO sign-off in [`CONTRIBUTING.md`](../CONTRIBUTING.md),
which is lighter than a CLA and is what the kernel, Git and Docker use.

If Swept ever grows a commercial tier, an enterprise customer base, or
corporate contributors, revisit this. Relicensing is cheap while there is one
copyright holder and expensive afterwards.

### The constraint that must not be broken

`TERMS.md` must never add a **restriction** to the MIT grant on the source. It
may add disclosures, acknowledgements and terms that apply to *our official
binaries*; it may not tell anyone what they can do with the code. Cross that
line and two things happen: Swept stops being open source in any meaningful
sense, and the contradiction between the two documents gives a plaintiff an
argument that neither controls.

This is why `TERMS.md` §0 exists and why it says the MIT License wins on
conflict. Keep that section intact.

## Why an LLC, and what it does not do

**What it does.** It is the party to the terms, the Apple Developer Program
member, the copyright holder, and the defendant. A user who loses data sues the
LLC, and the LLC's assets — which is to say, not the house — are what is
exposed. It also gives the app a publisher name that is not a personal legal
name, keeps program fees and any future revenue on their own books, and is the
natural home for a paid tier later.

**What it does not do.** Four honest limits:

- **It does not replace the disclaimer.** An entity limits *who* is liable, not
  *whether* anyone is. Sections 4 and 5 of `TERMS.md` do the second job, and
  they are what actually gets a case dismissed early.
- **It does not shield you from your own conduct.** Most US states let a
  plaintiff name the individual who personally committed a tort, even when they
  acted for an LLC. Sole-member software LLCs are named alongside their owner
  routinely. The shield is real for contract claims and business debts; it is
  softer for "the person who wrote the code did it negligently".
- **It only holds if you keep it separate.** Commingle personal and business
  money, skip the annual filing, sign in your own name instead of the
  company's, and a court can pierce the veil. The formalities are the product.
- **It is not free.** In Maryland: $100 to form, then **$300 every year** for
  the Annual Report / Personal Property Return, owed on zero revenue, plus
  $100–150/yr if you would rather not have your home address on SDAT's public
  business search. Miss the annual filing long enough and the charter is
  forfeited, which removes the shield entirely — the one failure mode here that
  is both cheap to avoid and total if ignored.

For a free tool the realistic probability of being sued is low. The LLC is
cheap insurance and a prerequisite for the Apple organization enrollment you
want anyway; it is not the main protection.

### Why Maryland and not Delaware

The received wisdom is to form in Delaware. It does not apply here, and the
arithmetic runs the other way.

**Delaware costs more, every year, forever.** Forming in Delaware does not stop
Swept from being developed and published from Maryland, so the LLC would be
doing business in Maryland and would have to register there as a foreign entity
anyway — $100, plus a Delaware certificate of good standing. After that you owe
*both* states annually:

| | Maryland only | Delaware + Maryland |
|---|---|---|
| Formation | $100 | $110 DE + $100 MD foreign registration |
| Annual, state 1 | $300 (Form 1, Apr 15) | $400 DE franchise tax (Jun 1) |
| Annual, state 2 | — | $300 MD Form 1 (Apr 15) |
| Resident/registered agent | optional | **mandatory** in DE, ~$50–300/yr |
| **Ongoing total** | **$300/yr** | **~$750–1,000/yr** |

Delaware's LLC franchise tax rose from $300 to **$400** — the state's own page
now states the higher figure, so the $300 quoted in most guides is stale.

**And it buys nothing this project needs.** Delaware's advantages are real but
specific: the Court of Chancery, a deep body of case law, and predictable
default rules for disputes *among owners and managers*. Those are internal
affairs. A single-member LLC with no investors, no co-founders and no board has
no internal affairs to litigate. The reason startups incorporate in Delaware is
that institutional investors require it, and there are no investors here.

**The risk you actually have is a tort claim from a user whose data went away,
and Delaware does not help with that.** The internal affairs doctrine sends
governance questions to the state of formation; it does not send a product
liability or negligence claim there. That claim is governed by the forum's
choice-of-law rules, which generally point at where the harm happened — the
user's state — regardless of where the LLC was filed.

**Maryland is, if anything, the better forum for the one question that matters.**
Where the state of formation *does* bear on the shield is veil-piercing, and
Maryland is among the most protective states in the country. Under *Bart
Arconti & Sons v. Ames-Ennis*, 275 Md. 295 (1975), the veil is pierced only "to
prevent fraud or enforce a paramount equity" — and Maryland's appellate courts
have never found a paramount equity sufficient on its own, so in practice fraud
is required. Forming at home costs less *and* lands you in a jurisdiction that
is hostile to the argument a plaintiff would need to make.

**Wyoming, Nevada and New Mexico fail the same way** — cheaper annual fees, but
you still register in Maryland, still pay Maryland, and still add a second
state's filings. The "$50/yr Wyoming LLC" is $50 *on top of* Maryland, not
instead of it.

The one thing that genuinely does not care about the state line is the
participation doctrine: Maryland, like most states, holds a member personally
liable for torts they personally commit, however the entity is organised. That
is why the entity is layer 5 of five, and the code is layer 1.

## Why not the Mac App Store

Two independent blockers, either sufficient:

1. **The sandbox.** App Store apps must be sandboxed and cannot read or write
   outside their container without a user-selected file grant per item. Swept
   enumerates other applications' caches, `~/Library/Logs`, Xcode derived data,
   browser profiles and `LaunchAgents`. There is no sandbox entitlement that
   makes that possible, and a per-file open panel for thousands of cache files
   is not a product.
2. **`macOSPrivateApi`.** The transparent window and `NSVisualEffectView`
   sidebar (roadmap U1) require Tauri's private-API flag, which forecloses App
   Store review on its own.

Cleaner utilities are also a category App Review treats with suspicion
independently of the above. CleanMyMac itself is distributed under Developer ID
rather than through the store, for the same reasons.

**Developer ID + notarization + a direct `.dmg` is the only viable channel**, and
it is what roadmap D2 already describes. See
[`docs/RELEASING.md`](RELEASING.md) for the mechanics.

One consequence worth internalising: with no App Store between you and the
user, **there are no platform terms doing any of this work for you.** Apple's
licence agreement is not standing in front of you. `TERMS.md` and the first-run
acknowledgement are the entire contractual surface, which is why they are
built rather than borrowed.

## Trademarks

The README calls Swept "an open-source alternative to CleanMyMac". That is
nominative fair use — using MacPaw's mark to refer to MacPaw's product in a
truthful comparison — and it is lawful. It stays lawful only while the use is
minimal and unmistakably unaffiliated, which is what the rules in
[`NOTICE.md`](../NOTICE.md) are for.

**Before the first public release**, run a knockout search for "Swept" in
class 9 (computer software) on the [USPTO TESS
database](https://tmsearch.uspto.gov/) and a plain web search. The name is a
common English word, which cuts both ways: harder for anyone to own broadly,
easier for someone to already be using it. Registration is optional — common-law
rights attach from use — but the search is not, because discovering a conflict
after the Developer ID, the bundle identifier `net.chandler.swept`, the domain
and the icon are all built is the expensive version.

## Shipping as an individual, on purpose

**Swept is published by a person, not a company, and is distributed unsigned.**
That is a decision with a rationale, not a half-finished migration, and it is
written down here so nobody "fixes" it by accident.

**Why it is defensible.** Layer 5 of the five above is the entity, and it is the
weakest of them. It limits *who* a loss can reach; it does not decide whether
anyone is liable at all. Layers 1 to 4 — the preview, the consent design, the
disclaimer, and the acceptance record — are what would actually get a claim
dismissed, and all four exist today. Nearly all open-source software is
published by individuals with no entity whatsoever; that is the ordinary case,
not a compromise.

**What makes the exposure small in practice.** Swept is free, so there is no
contract of sale, no implied warranty of merchantability in most consumer
regimes, and a much weaker basis for damages. It makes no performance claims —
`CONTRIBUTING.md` bans marketing language outright. It previews before it acts,
prefers the Trash to deletion, and writes down what it was told to do. A claim
would have to get past a disclaimer the user actively accepted, prove a defect
and causation, and overcome the fact that the same screen told them to keep a
backup. That is an unattractive case with a shallow pocket behind it.

**What it costs.** Personal assets are exposed if a claim ever succeeded, and
users see a Gatekeeper warning on every download because the builds are
unsigned. Both are real; neither is worth $99/yr plus formation costs *yet*.

**Why not enroll with Apple as an individual in the meantime.** Because it is a
one-way door pointed the wrong way. Apple issues a **new Team ID** when an
individual converts to an organization, which means a new Developer ID
certificate, which means macOS treats the next release as a *different
developer*: users who allowed the old build are prompted again, and a Homebrew
cask's verification stanza changes. Enrolling now buys a migration that would
have to be undone. And a DBA or sole proprietorship does not help — Apple
treats sole proprietors as **individual** enrollment, and a trade name provides
no liability protection at all. It is a naming device, not a shield.

**What should trigger revisiting this**, in rough order of how strongly:

1. **Money.** A paid tier, donations tied to features, or any sale. This is the
   big one: it creates a contract of sale and pulls in consumer-refund law and
   sales-tax nexus at the same time.
2. **Deployment at organisational scale**, or anyone running Swept across
   machines they do not personally own.
3. **Wanting the signed, notarized experience** enough to pay for it — at which
   point form the entity *first*, because of the Team ID problem above.
4. **Outside contributors with real copyright stakes**, which makes relicensing
   and assignment expensive rather than cheap.

Until one of those, the honest posture is the current one, and the documents
say so plainly rather than leaving a blank where a company would go.

## The open questions, tracked

- [ ] Attorney review of `TERMS.md` and `PRIVACY.md` — a Maryland attorney,
      now that the governing law is Maryland.
- [x] Governing law resolved to Maryland — see "Why Maryland and not Delaware"
      above.
- [x] Publisher named throughout, with no placeholders left anywhere. Swept
      ships under an individual's name; `scripts/verify.sh` fails if `LICENSE`
      and the four documents that repeat it ever drift apart.
- [ ] Tech E&O / professional liability quote. For a free tool the premium is
      small, and it is the only layer here that *pays* rather than merely
      capping exposure — the entity limits what a loss can reach, insurance is
      what answers the loss.
- [ ] Trademark knockout search for "Swept" in class 9.
- [ ] Copyright line in `LICENSE` updated to the entity — **if** one is ever
      formed. Changing it there is enough; the gate then requires the other
      four to follow.
- [ ] Decide whether the repository goes public at launch (MIT already assumes
      the source is available; a private repo with a public binary is a
      coherent but different posture).
- [ ] Revisit if a paid tier appears: consumer-refund law, sales-tax nexus and
      an EULA with payment terms all become relevant at once.
- [ ] Technology E&O insurance is **not** worth it for a free tool — the premium
      exceeds the expected loss by a wide margin. It becomes worth pricing if
      Swept is ever sold, deployed under contract, or used at organisational
      scale, because at that point a single claim has a plausible path to real
      damages. Noted here so the question is answered deliberately rather than
      never asked.
