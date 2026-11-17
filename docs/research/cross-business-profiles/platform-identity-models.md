# Platform identity models: one consumer login across many independent merchants

Researched 2026-11-12. Sources are marked [primary] (the vendor's own help or developer docs), [press], [community], or [weak] (competitor marketing, review aggregators, SEO blogs). Many pages were read through search summaries rather than full fetches; those are marked (snippet).

## Shop Pay and Sign in with Shop (Shopify)

**Takeaway:**
Shopify runs a person-owned Shop identity above each store's own customer record. Login with Shop is a standard OpenID Connect flow that gives the store only a stable public ID and an email, and it creates or matches a store-level customer record. Saved address and card stay on Shopify's servers and reach the store only when an order is placed. Merchants get it on by default once Shop Pay and customer accounts are on.

**Cited Findings:**

- Shop Pay saves the buyer's email, payment method, and shipping and billing details on Shopify's PCI compliant servers. "This information is only shared with your store when an order is placed." [primary] — [Shopify Help: Shop Pay](https://help.shopify.com/manual/payments/shop-pay)
- A buyer must enter an email at checkout to be offered saving; a phone number alone does not trigger the prompt. [primary] — [Shopify Help: Shop Pay](https://help.shopify.com/manual/payments/shop-pay)
- Sign in with Shop "is automatically activated on stores using Shop Pay and customer accounts." Buyers who "already opted in to Shop Pay on any Shopify store" get faster checkout elsewhere. Buyer-side enrollment is opt-in at a checkout; merchant-side it is on by default. [primary] — [Shopify Help: Shop Pay](https://help.shopify.com/manual/payments/shop-pay); [Changelog: log in with Shop added to new customer accounts](https://changelog.shopify.com/posts/sign-in-with-shop-added-to-new-customer-accounts) (snippet)
- "Merchants will always have access to the email address associated with the Shop account used to sign in on their customer record." [primary] — [Changelog](https://changelog.shopify.com/posts/sign-in-with-shop-added-to-new-customer-accounts) (snippet)
- A first-time Shop login at a store automatically creates a customer account on that store (classic accounts), or a new customer profile if no email matches (new accounts). [primary] — [Changelog: classic customer accounts](https://changelog.shopify.com/posts/sign-in-with-shop-added-to-classic-customer-accounts); [Shopify Help: customer accounts](https://help.shopify.com/manual/customers/customer-accounts) (snippet)
- Developer flow: the store requests `scope=openid email`, receives an `id_token` with `sub` (a Shop public ID) and `email`, verifies it against JWKS, then looks up the customer record by stored public ID, else creates one "from sub and email." Payment receipts carry a nullable `shopUser.publicId`. [primary] — [shopify.dev: Identify Shop users](https://shopify.dev/docs/api/commerce-components/pay/identify-shop-users)
- Shopify's Feb 2023 merchant blog: Shop cookies "immediately identify Shop users on brand storefronts"; users log in with Shop credentials "including passkeys"; payment details and addresses "are automatically pre-filled at checkout"; claims Shop Pay users sign into customer accounts "up to 50% more" with Sign in with Shop. [vendor marketing] — [Shopify blog, 2023-02-09](https://www.shopify.com/blog/shop-sign-in-features)
- Shopify's customer privacy stance: the merchant "legally controls your information," so Shopify forwards a buyer's access or deletion request to the merchant. [primary] — [Shopify customer privacy policy](https://shopify.com.au/legal/privacy/customers) (snippet)
- Shopify added an option (March 2026) to enrich customer records with data from external identity providers. [primary] — [Shopify changelog 2026-03-17](https://changelog.shopify.com/posts/enrich-customer-records-with-data-from-external-identity-providers) (title only; not read)
- Merchant complaint example: a merchant found a buyer had paid with Shop Pay though the merchant "never OK'd" it. [community] — [Shopify Community thread](https://community.shopify.com/t/i-never-okd-a-shop-pay-option-in-the-store-customer-just-used-it-will-merchant-be-paid-fully/266687)
- Press concern on data flowing beyond the merchant: Shop Pay on Google shares card, billing and address data with Google. [press] — [Digiday](https://digiday.com/retail/cheat-sheet-shopifys-shop-pay-integration-will-share-customer-purchase-data-with-google)

**Inferences:**

- This is the clearest "platform identity above per-merchant accounts" model: the person owns the Shop identity; each store owns its own customer record keyed by the Shop public ID; profile data reaches a store only when needed (an order), and identity disclosure is minimal (ID plus email).
- Merchants accept it because the store still gets a normal customer record and the email, and Shopify frames the merchant as data controller.

**Gaps:**

- No primary page found on one-time codes for Shop login, the exact buyer consent screen, or a merchant toggle to disable Sign in with Shop alone.
- Did not find substantive organized merchant backlash; the Shop app's own marketing to buyers (cross-store recommendations) was not researched.

## Stripe Link

**Takeaway:**
Link is a person-owned wallet recognized across every Stripe merchant by email plus an SMS one-time code. Merchants get the transaction data for their own payment; bank details need separate consent. Merchant-side it is on by default in the Payment Element, with "accelerated sign-up" also on by default.

**Cited Findings:**

- "You can autofill information for any logged-in customer already using Link, regardless of whether they initially saved their information in Link with another business. The default Payment Element integration includes a Link prompt in the card form." [primary] — [Stripe docs: Link in the Payment Element](https://docs.stripe.com/payments/link/payment-element-link)
- Returning users authenticate with a 6 digit one-time passcode. Prefilled values "are never stored unless the customer completes a Link sign-up." [primary] — [Stripe docs](https://docs.stripe.com/payments/link/payment-element-link)
- Accelerated signup, which opts customers into saving details to Link by default, "is enabled by default"; "Link disables or limits this feature when local regulations prohibit it. Customers can opt out at any time." The prefill tool is on by default and can be disabled in the Dashboard. [primary] — [Stripe docs](https://docs.stripe.com/payments/link/payment-element-link)
- Merchant receives "Transaction Data that includes your Personal Data and, with your separate consent, your bank account information." [primary] — [Link privacy policy](https://link.com/privacy-prior) (snippet)
- For linked bank accounts, Stripe shows what the merchant requested and the user must consent; the consent pane "will not be modifiable by you or other merchants"; new data types need a new Stripe consent flow. [primary] — [Stripe: Financial Connections merchant guidelines](https://support.stripe.com/questions/financial-connections-merchant-guidelines) (snippet)
- Merchants can turn Link off in payment method settings; third-party help shows merchants doing so. [primary, community] — [Stripe docs](https://docs.stripe.com/payments/link/payment-element-link); [Stora help: disable Link](https://intercom.help/stora/en/articles/9522953-disable-link-payments-in-stripe)

**Inferences:**

- Pattern: platform-held payment credentials, per-merchant disclosure only at transaction, and a platform-owned consent screen merchants cannot alter. Separate consent per merchant for sensitive extras.

**Gaps:**

- No primary source on merchant complaints about Link; anecdotes exist of merchants disabling it, but reasons were not sourced.

## Mindbody Universal Account (2025)

**Takeaway:**
Mindbody always had a consumer-level Mindbody account plus a separate client profile in each studio's database, linked by matching name and email. Around July 31, 2025 it moved to a Universal Account that clients must sync to studio profiles; unsynced clients could not log in or book. The claim of operator backlash and partial rollback rests on one competitor blog and was not confirmed by primary or press sources.

**Cited Findings:**

- Mindbody's FAQ: the account works across many businesses with one email and password, while each business keeps its own local client database; a client links the account to local profiles by clicking an emailed link; studio signup requires the same name and email as the Mindbody account. [primary] — [Mindbody account FAQ](https://support.mindbodyonline.com/s/article/I-already-have-an-account?language=en_US) (snippet; page did not render on fetch)
- Studio staff cannot edit a linked client's name or email; only the client can, at account.mindbodyonline.com or in the app. The Mindbody account section does not show on unlinked profiles. [primary] — [Mindbody: Client profile screen](https://support.mindbodyonline.com/s/article/201067303-Client-profile?language=en_US) (snippet)
- A studio notice: "as of July 31, 2025, Mindbody ... introduced a Universal Account system," and "many clients have encountered temporary issues signing in if their studio accounts and universal accounts aren't synced." The fix is a sync banner at account.mindbodyonline.com, or linking the studio under "Places You Go"; passes, credits, and history carry over. [studio, secondary] — [Bay Studios](https://bay-studios.com/mindbody-universal-account-update/)
- Before the change, a common consumer complaint was a separate account per studio ("Why does MindBody create separate accounts for every studio?"). [community, via competitor summary] — [Vibefam](https://vibefam.com/mindbody-reviews-reddit-2026/), citing [r/yoga](https://www.reddit.com/r/yoga/comments/1mgo5sh/why_does_mindbody_create_separate_accounts_for/) (not fetched)
- Claim: mid-2025 "Universal Login" routed studio customers "through Mindbody-controlled identity rather than studio-side records," and was "partially rolled back after public backlash." [weak: competitor blog, single source] — [Vibefam review](https://vibefam.com/mindbody-review-pricing-features-pros-cons-2026/) (snippet)
- Operator complaints on ownership: Mindbody "believes studio customers are Mindbody customers"; a studio was charged a cut on a client who found it independently but booked in the Mindbody app; a "new client discovery fee" introduced with limited notice. [community, weak aggregation] — [Trustpilot](https://pl.trustpilot.com/review/mindbodyonline.com) (snippet); [Vibefam](https://vibefam.com/mindbody-reviews-reddit-2026/), citing [r/mindbody](https://www.reddit.com/r/mindbody/comments/1pde1yv/did_you_know_about_the_new_new_client_discovery/)
- Mindbody and ClassPass sit under one parent brand, Playlist, since June 2025. [weak: Wikipedia] — [Wikipedia: ClassPass](https://en.wikipedia.org/wiki/ClassPass)
- Mindbody's ClassPass FAQ commits that "Neither Mindbody nor ClassPass will be marketing to your customers directly using your data." [primary] — [Mindbody: how ClassPass works for businesses](https://www.mindbodyonline.com/business/education/blog/how-classpass-works-for-businesses) (snippet)

**Inferences:**

- The failure mode was migration: forcing link-up of existing per-studio profiles to a platform identity broke login for unsynced clients. Matching by exact name and email is brittle.
- Operator distrust centers on the platform also running a consumer marketplace that charges acquisition fees, so a shared identity reads as the platform claiming the customer.

**Gaps:**

- No Mindbody announcement, press coverage, or confirmed rollback found. Reddit could not be fetched.

## ClassPass, Booksy, Vagaro, Fresha

**Takeaway:**
These are marketplace accounts: the consumer has one platform account across all businesses. Merchant objections focus on commission on "new" clients they believe came from elsewhere, and on difficulty exporting client data. Sources are mostly competitor blogs.

**Cited Findings:**

- Fresha marketplace takes about 20% of a new client's first booking ($6 minimum); Booksy Boost 30% of the first visit, capped at $100; Vagaro marketplace listing free. [weak: competitor table] — [Primzy](https://www.primzybeauty.com/blog/the-true-cost-of-7-salon-booking-platforms-and-the-one-with-a-no-commission-booking-link-x36hoFx4)
- Owner complaint (June 2026): "9 out of 10 of my 'new clients' actually come from Google, not Fresha." [weak] — [SortTheClicks](https://sorttheclicks.com/fresha-reviews-reddit/)
- Fresha reportedly retains control of uploaded client data, making export hard; an owner was told marketplace visibility required sharing existing client emails. [weak] — [ExpressBook](https://expressbook.com/fresha-reviews/); [TimeTailor](https://www.timetailor.com/timetailor-alternatives/fresha-reddit-reviews)
- A Vagaro skin studio owner said chart notes and photos sitting in Vagaro made switching hard. [weak] — [SortTheClicks](https://sorttheclicks.com/fresha-reviews-reddit/)
- ClassPass added partner protections after studio complaints, including a "Mindbody Direct Protection" tool to stop recent direct customers being poached via trials. [press] — [Athletech News](https://athletechnews.com/classpass-moves-to-ease-studios-complaints-new-partner-policies-protections/) (snippet)
- ClassPass claims 94% of its users are new to the venues they visit. [vendor] — [ClassPass 2026 Industry Impact Report](https://classpass.com/partners/2026-industry-impact-report) (snippet)

**Inferences:**

- Merchant objection is less about login and more about attribution and control: a shared consumer account becomes a channel the platform can charge for and market through.

**Gaps:**

- No primary terms on data ownership for Fresha, Booksy, or Vagaro; no detail on whether Booksy or Vagaro consumer accounts pre-fill details at a new business.

## Campground platforms

**Takeaway:**
Hipcamp and Campspot are marketplaces with one camper account across parks. Hipcamp saves RV type and length to the profile for future bookings. Campspot's commission is about 10% and its guest data ownership is disputed: Campspot says parks own marketplace guest data; competitors say Campspot keeps and remarkets it.

**Cited Findings:**

- Hipcamp: pick an RV type, enter length including tow vehicle, and "these specifications are saved to your profile so you only have to enter them once!" [primary vendor] — [Hipcamp Journal](https://www.hipcamp.com/journal/camping/why-rvers-use-hipcamp)
- RoverPass is park software; a Hipcamp booking appears as an inventory hold on the park's RoverPass calendar, and Hipcamp collects payment and pays out the park. [primary] — [RoverPass help: Hipcamp](https://help.roverpass.com/campground/hipcamp)
- Campspot account holds saved campgrounds and reservations across parks. [primary] — [Campspot camp guide](https://www.campspot.com/camp-guide/10-most-saved-campgrounds-on-campspot); [Campspot FAQ](https://www.campspot.com/faq)
- Campspot marketplace: 10% commission, no setup fees; marketing says "all customer data from Marketplace reservations is rightfully yours." [primary, via snippet] — [Campspot Marketplace intro](https://support.campspot.com/campspot-marketplace) (login-gated on fetch)
- Contrary claims: Campspot "ultimately owns that guest data," limiting remarketing; the directory "collects the guest information so that the Campspot OTA directory can market to your campers in the future." [weak: competitors] — [KeeprStay](https://keeprstay.com/compare/campspot); [ResNexus](https://www.resnexus.com/comparisons/campspot.html)
- Spot2Nite lets campers save RV equipment details and club memberships in a profile. [weak] — [RV Love](https://rvlove.com/rv-camping/book-campgrounds-easier-and-faster-with-spot2nite/)
- Recreation.gov "Vehicle Profile" claim is from an unverified RV dealer blog. [weak] — [Trailerite](https://trailerite.com/recreation-gov-reservation-tips-new-booking-rules.html)

**Inferences:**

- Vehicle data is the profile item campers value most for re-use, and parks need it to assign sites. Parks' worry is the marketplace remarketing to their guests and sending them to competitors in the same directory.

**Gaps:**

- Not researched or not found: KOA rewards account model, ReserveAmerica account across agencies, official Recreation.gov profile fields, whether Campspot passes saved vehicle details to the park.

## Restaurants: OpenTable, Resy, Toast

**Takeaway:**
OpenTable is one diner account across restaurants, with explicit "restaurants own their data, guests own their data" language and per-booking sharing of name, contact, party size, and requests. Toast has no cross-restaurant guest account with saved card; loyalty is per restaurant or restaurant group, and the Toast Local app lists each one.

**Cited Findings:**

- OpenTable: "Restaurants own their data, and guests own their data. Any data entered by a restaurant into their OpenTable system is owned by the restaurant." The diner grants OpenTable the right to share with restaurants, limited by what scope the diner agreed to. [primary] — [OpenTable Guest Data & Security](https://restaurant.opentable.com/?p=80424) (snippet)
- OpenTable "shares your name, contact details, party size and special requests with the restaurant you book"; US guest emails are shared with marketing opt-in status. [primary/press] — [OpenTable privacy policy](https://www.opentable.com/legal/privacy-%20policy); [Fox News](https://www.foxnews.com/tech/how-restaurant-reservation-platform-opentable-tracks-customer-dining-habits) (snippet)
- Ownership fight: OpenTable restricted third-party access and charged rival SevenRooms clients up to $1,000 per restaurant for OpenTable diner data; restaurateurs saw it as boxing out SevenRooms. [press] — [The Hustle](https://thehustle.co/opentable-reservation-tech-sevenrooms); [Hospitality Tech](https://hospitalitytech.com/debate-who-owns-restaurants-data-heats-opentable-makes-changes)
- Toast: guests sign up separately for each rewards program; a group can share one loyalty account across its locations; the Toast Local app shows every restaurant where a guest has a rewards account. [primary] — [Toast: Getting started with Toast Loyalty](https://support.toasttab.com/en/article/Getting-Started-Toast-Loyalty); [Toast: Look up loyalty balance](https://support.toasttab.com/en/article/Looking-Up-a-Toast-Loyalty-Rewards-Balance) (snippets)
- Toast card linking: Toast "does not have access to any transaction information associated with your linked card from outside of Toast restaurants." [primary] — [Toast Guest Support: Credit Card Linking FAQ](https://toasttakeout.zendesk.com/hc/en-us/articles/45364479629075-Credit-Card-Linking-FAQ)

**Inferences:**

- Toast's model (per-merchant programs, one consumer app that aggregates them) is the merchant-comfortable end; OpenTable shows explicit dual ownership language as a way to calm restaurants.

**Gaps:**

- Resy not researched. No source on whether a Toast guest's saved card carries across unrelated restaurants for online ordering.

## Airbnb (extreme marketplace)

**Takeaway:**
Airbnb owns the guest relationship. Hosts get no real email, masked or limited phone, and are barred from soliciting contact details. Hosts object on safety and direct-booking grounds.

**Cited Findings:**

- Airbnb strips contact details from messages before booking; it stopped sharing even anonymized alias emails (via @guest.airbnb.com) and shut down the alias feature. [weak/community] — [Zeevou](https://zeevou.com/blog/no-email-alias-to-your-airbnb-guests-do-you-really-need-it/); [Skift](https://skift.com/?p=604776)
- Airbnb masks phone numbers with temporary numbers for some US and Canadian users; reports say hosts are barred from soliciting phone, address, or email. [weak] — [Rental Scale-Up](https://www.rentalscaleup.com/airbnb-temporary-phone-numbers/)
- Hosts complain they cannot get guest email for safety or records. [community] — [Airbnb Community](https://community.withairbnb.com/t5/Support-with-your-bookings/Airbnb-not-providing-renter-s-email-address-to-owner-manager/m-p/2061342)

**Inferences:**

- Simplest consumer experience, least merchant comfort. The opposite of a SaaS where each business is the customer of record.

**Gaps:**

- Dates of Airbnb's policy changes not confirmed from Airbnb's own pages.

## Cross-platform patterns

**Takeaway:**
The simplest consumer model is a marketplace account (Airbnb, Hipcamp, ClassPass). The model that keeps merchants comfortable while giving one login is Shopify's: a person-owned platform identity that each merchant sees as an ordinary customer record in its own database, with minimal identity claims and saved details released only at transaction time.

**Cited Findings:**

- "Link once, record per merchant": Shop creates a store customer record from `sub` and `email` on first login. [primary] — [shopify.dev](https://shopify.dev/docs/api/commerce-components/pay/identify-shop-users)
- "Share at transaction": Shop Pay details reach the store only on order; Link sends transaction data per payment. [primary] — [Shopify Help](https://help.shopify.com/manual/payments/shop-pay); [Link privacy](https://link.com/privacy-prior)
- "Platform-owned consent screen per merchant" for sensitive data. [primary] — [Stripe Financial Connections guidelines](https://support.stripe.com/questions/financial-connections-merchant-guidelines)
- "Client owns identity fields, merchant owns the rest": Mindbody locks name and email to the client. [primary] — [Mindbody client profile](https://support.mindbodyonline.com/s/article/201067303-Client-profile?language=en_US)
- "Explicit dual ownership statement" (OpenTable). [primary] — [OpenTable](https://restaurant.opentable.com/?p=80424)

**Inferences:**

- For Tetsu: a person-owned profile that pre-fills a new business's customer account, keeps the business account as the record of truth, sends vehicle and payment only on booking, and never powers cross-business marketing or commissions would sit on the Shopify side, away from the Mindbody and Campspot complaints.
- Avoid forced migration of existing per-business accounts into the profile; Mindbody's sync breakage shows the cost. Make linking optional and per business.
- Payment cards across merchants are usually held by the payment provider (Shop Pay, Link), not the booking SaaS.

**Gaps:**

- No quantitative data on merchant opt-out rates for Sign in with Shop or Link.
