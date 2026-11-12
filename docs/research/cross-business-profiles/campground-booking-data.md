# Campground and RV park booking data: camper profile versus park records

Researched 2026-11-12. Sources are web search results and fetched pages. Many campground vendor help centers (Campspot, Hipcamp, Pitchup) blocked direct fetching (login redirect or HTTP 403), so some Campspot and Hipcamp facts come from search result summaries of those pages, not full reads. Those are marked "(search summary)". Vendor marketing and competitor comparison pages are marked "(vendor marketing)".

## What campground reservation systems collect per booking and per guest

**Takeaway:**
Campground bookings need far more physical data than a class or salon booking: equipment type, rig length (sometimes with slide-outs and tow vehicle), electrical amperage, vehicle count, license plates, and occupant count. Sites carry matching limits, and a mismatch at arrival can void the stay, so this data drives site fit, not just record keeping.

**Cited Findings:**
Equipment type and length

- Recreation.gov campsite pages list each permitted equipment type with its own maximum length, for example "Tent, RV (Max length: 24 ft.), Trailer (Max length: 12 ft.)" — [Recreation.gov campsite](https://recreation.gov/camping/campsites/88681) (search summary)
- Yosemite tells campers to "Select the type(s) of equipment you have" in search filters, and says tent sites have no room for RVs and RV sites have no room for tents — [NPS Yosemite: How to Make a Campground Reservation](https://home.nps.gov/yose/planyourvisit/nrrs.htm)
- Yosemite warns: "If you arrive with different equipment than allowed in the site you reserved, we will not be able to accommodate you." — [NPS Yosemite camping FAQ](https://www.nps.gov/yose/planyourvisit/campingfaq.htm) (search summary)
- One Recreation.gov listing defines trailer length as "Trailer + Hitch + Vehicle". Another note says maximum trailer length is always shorter than maximum RV length because of maneuverability — [Recreation.gov campsite](https://recreation.gov/camping/campsites/88681) (search summary)
- Hipcamp counts the towed vehicle along with the driving vehicle when comparing length to a site limit — [Hipcamp: How is vehicle length calculated](https://support.hipcamp.com/hc/en-us/articles/32224213213076-How-is-vehicle-length-calculated) (search summary; page returned 403)
- Definitions differ by park. A KOA Jackson listing says "We only need the length of your RV when making a reservation. No need to include your tow vehicle." — [KOA Jackson RV sites](https://koa.com/campgrounds/jackson/site-type/rv-camping-sites/) (search summary). This contradicts the Hipcamp and Recreation.gov rule above, so length must be stored as components (rig, tow vehicle) rather than one number.
- Some KOA booking forms ask a required yes/no "With Slideouts?" question beside length — [KOA Crawfordsville RV sites](https://koa.com/campgrounds/crawfordsville/site-type/rv-camping-sites/) (search summary)
- Campspot site setup and site reports carry minimum and maximum length, RV types allowed, and slide-outs per site — [Campspot: Filling out the site template](https://support.campspot.com/filling-out-the-site-template); [Campspot: Campsite detail report overview](https://support.campspot.com/campsite-detail-report-overview) (search summary)
- Resort Data Processing (IRM) stores a coded rig type, for example code "W" for "Trailer, 4th Wheel, Pop-up Tent Trailer" — [Resort Data: Campland preferences](https://support.resortdata.com/Support/Content/KB/CustomFeatures/CamplandPreferences.htm) (search summary)
- Sharper MMS booking portal filters sites by length, beam, and height — [Sharper MMS: Campground online booking portal](https://support.sharpermms.com/en/articles/14502049-campground-online-booking-portal) (search summary)
- Parkwith collects rig type and length to compute extra fees and filter sites, and supports custom fields such as license plate — [Parkwith help: online reservations](https://help.parkwith.us/reservations/9JpMNAQcVQmoAqFqBgXFY7/online-reservations-for-your-campers/5aKc8YAJHR9UFesFY58835) (search summary)

Hookups

- KOA sites come in 20, 30, and 50 amp options. KOA Jackson states "NO 50 AMP RV's ALLOWED ON 30 AMP SITES" — [KOA Jackson RV sites](https://koa.com/campgrounds/jackson/site-type/rv-camping-sites/) (search summary)

Vehicles and plates

- Recreation.gov listings show maximum vehicles per site. One allows one vehicle and says "Any additional vehicles must be paid for within 30 minutes of arrival." Another allows two vehicles and six people — [Recreation.gov campsite 88681](https://recreation.gov/camping/campsites/88681); [Recreation.gov campsite 993](https://recreation.gov/camping/campsites/993) (search summary)
- A Recreation.gov campground charges $10 per vehicle beyond the two included — [rec.gov campground 10165235](https://rec.gov/camping/campgrounds/10165235) (search summary)
- Hipcamp hosts set a vehicle limit per booking. Guests pick vehicle count on the final booking page, and some hosts charge an extra-vehicle add-on — [Hipcamp: How many vehicles can I bring](https://support.hipcamp.com/hc/en-gb/articles/20520146972052-How-many-vehicles-can-I-bring) (search summary)
- ReserveCalifornia lets campers update vehicle information on upcoming reservations until 48 hours before arrival, with "no additional fees", and staff accept vehicle changes at check-in — [ReserveCalifornia FAQs](https://www.parks.ca.gov/?page_id=29676)
- Innroad places reservation custom fields under a vehicle info section, with up to 10 text or dropdown custom fields — [Innroad: Field configurations](https://support.innroad.com/support/solutions/articles/153000146034) (search summary)

Party size and checkout flow

- Recreation.gov checkout asks for RV information, group size, and vehicle count — [RVshare: How to book campgrounds in national parks](https://rvshare.com/blog/how-to-book-campgrounds-in-national-parks/) (third-party guide)
- Alberta Parks checkout asks for equipment type, occupant count, and vehicle details before adding a site to the cart — [Alberta Parks: How to book a campsite (PDF)](https://cms.albertaparks.ca/media/6497730/how-to-book-a-campsite.pdf) (search summary; not Recreation.gov, shown as an analogous public system)
- Yosemite check-in and check-out are both noon — [NPS Yosemite camping FAQ](https://www.nps.gov/yose/planyourvisit/campingfaq.htm) (search summary)

Custom fields

- Campspot lets operators add custom plain-text fields to reservation summaries and guest profiles, and warns that too many required fields slow booking — [Campspot: Additional reservation and guest information fields](https://support.campspot.com/additional-reservation-and-guest-information-fields) (search summary; page needs login)

**Inferences:**

- A practical inventory for one campground booking: equipment type, rig length, slide-outs, tow vehicle and its length, amperage need, water and sewer need, number of vehicles, each plate, adults and children, pets, arrival time, accessibility need, site preference. Most of these were confirmed above. Pets, ages, accessibility, arrival times, and site preference I could not confirm on a primary page (see Gaps), though they are common on operator forms.
- Site attributes mirror rig attributes (length, amps, slide-out room, vehicle count). Booking logic is a fit check between the two. This is unlike fitness or salons, where fit is a time slot and a staff member.
- Because parks disagree on what "length" means, a reusable profile should store raw facts (rig length, tow vehicle length, slide-out count) and let each park compute its own number.

**Gaps:**

- No primary page found for Newbook, ResNexus, RMS, Campground Master, or Firefly booking-form field lists for pets, ages, accessibility, arrival time, or site preference. Campspot help pages require login.
- Exact Recreation.gov checkout field labels were not confirmed from a Recreation.gov help page.
- No source found on water and sewer need as a captured booking field; it appears as a site attribute rather than a guest input in the sources seen.

## Which items are stable camper facts, per-stay choices, or park records

**Takeaway:**
Rig, tow vehicle, plates, pets, and contact details are stable camper facts that change only when the camper buys a new rig or car. Dates, party, which vehicles come, and site choice are per stay. Ratings, bans, internal notes, credits, and site assignment are park records, and vendors already keep them separate from what the guest sees.

**Cited Findings:**

- Firefly's guest profile holds name, organization, email, address, phone, and emergency contact; "Guest notes" that are internal and hidden from guests; a 1 to 5 guest rating with a "Banned" option that blocks online reservations and alerts staff; reservation credits; a marketing export checkbox; a Guest Vehicles tab where guests or staff add vehicles; and reservation history — [Firefly: Understanding the Guest Profile](https://fireflyreservations.com/knowledge-base/guest-profile?hsLang=en)
- Campspot's staff guest profile has an RV Profile section. RVs entered at booking are added to it automatically, and staff can add, edit, or delete RV profiles. The guest profile is a global account linked to several reservations — [Campspot: Booking guest profile](https://support.campspot.com/booking-guest-profile) (search summary; page needs login)
- Sharper MMS keeps a registry of customer-owned RVs with dimensions, insurance information, and make and model — [Sharper MMS: Vehicles](https://help.sharpermms.com/en/articles/13837380-vehicles) (search summary)
- Big Rig Media (Indio) documents a staff task to update guest equipment details — [Big Rig Media: How to update guest equipment details](https://kb.bigrigmedia.com/indio/how-to-update-guest-equipment-details) (search summary)
- ReserveCalifornia keeps plates on the profile: "We are not able to remove any license plate numbers from your profile", yet "You will need to input a new license plate number when booking your next reservation" — [ReserveCalifornia FAQs](https://www.parks.ca.gov/?page_id=29676)

**Inferences:**
Proposed split, based on the findings above:

| Item | Camper profile (stable) | Per-stay choice | Park record |
| --- | --- | --- | --- |
| Name, email, phone, address | Yes | | Copy at booking |
| Emergency contact | Yes | | Copy at booking |
| Rig: type, make, model, year, length, slide-outs, amperage | Yes, can be several rigs | Which rig comes | |
| Tow or toad vehicle, length | Yes | Whether it comes | |
| Vehicles and plates | Yes, several | Which come, count | Extra vehicle fees |
| Pets: species, breed, count | Yes | Which come | Pet fees, breed rules |
| Accessibility needs | Yes | | |
| Adults, children, ages | Household members could be stored | Who comes | |
| Dates, arrival time, site preference | | Yes | |
| Site assignment | | | Yes |
| Internal notes, rating, ban | Never shown to camper | | Yes |
| Credits, loyalty points, seasonal contract | | | Yes |
| Signed park rules or waiver | | | Yes, per park and version |
| Insurance on rig (long-stay) | Maybe | | Park copy needed |

- Bans and ratings must never leave the park that wrote them. Firefly hides notes from guests, which shows vendors already treat these as park-private.
- Plate history is a privacy issue: ReserveCalifornia cannot delete plates from profiles. A camper-owned profile should let the camper delete a vehicle, while each park keeps its own copy tied to past stays.

**Gaps:**

- No source on how vendors handle seasonal or monthly guests' long-term records (contracts, meters) beyond Sharper's long-term reservation pages, which I did not read in full.
- No source on campground waiver practice specifically.

## How existing platforms let campers save and reuse rig and vehicle details, and how parks see it

**Takeaway:**
Saved rig profiles are common in RV trip apps and some booking systems, but mostly as per-platform accounts. Campspot and Firefly store rigs and vehicles on a guest profile the park staff can edit. I found no platform that shares a camper-owned rig profile across independent parks on different software.

**Cited Findings:**

- Campspot auto-adds RVs entered at booking to the guest's RV Profile, and staff manage it — [Campspot: Booking guest profile](https://support.campspot.com/booking-guest-profile) (search summary)
- Firefly guests and staff can both add vehicles to the profile; staff can also delete them — [Firefly: Understanding the Guest Profile](https://fireflyreservations.com/knowledge-base/guest-profile?hsLang=en)
- Campendium lets users save an RV to their profile, and save several RVs — [Campendium: How do I update the RV on my profile](https://support.campendium.com/hc/en-us/articles/49125861643924-How-Do-I-Update-the-RV-on-My-Profile); [Campendium: Can I save multiple RVs](https://support.campendium.com/hc/en-us/articles/49126540970772-Can-I-Save-Multiple-RVs-on-My-Profile)
- RV LIFE keeps an "RV Garage"; Roadtrippers has "Adding and Editing Vehicles"; OutReserve supports adding an RV to a profile — [RV LIFE: Accessing your RV Garage](https://support.rvlife.com/hc/en-us/articles/35850744799639-Accessing-Your-RV-Garage); [Roadtrippers: Adding and editing vehicles](https://support.roadtrippers.com/hc/en-us/articles/22035619195412); [OutReserve: How to add an RV to your profile](https://outreserve.com/help/how-to-add-an-rv-to-your-profile)
- RV Overnights asks members to set up a member profile before requesting a stay — [RV Overnights help](https://intercom.help/rvovernights/en/articles/10097125-set-up-your-member-profile-before-requesting-a-stay)
- Recreation.gov's app describes account access for reservations and favorite locations. I found no Recreation.gov help page on saving equipment to a profile — [Recreation.gov mobile app](https://recreation.gov/mobile-app)
- ReserveCalifornia stores plates on the profile but requires re-entry each booking — [ReserveCalifornia FAQs](https://www.parks.ca.gov/?page_id=29676)
- KOA Rewards members get a discount, often 10% at checkout, and some parks offer double points — [KOA Jackson RV sites](https://koa.com/campgrounds/jackson/site-type/rv-camping-sites/); [KOA Indianapolis West RV sites](https://koa.com/campgrounds/indianapolis-west/site-type/rv-camping-sites/) (search summary)

**Inferences:**

- The demand for a saved rig is proven by many apps, but each keeps its own copy. A cross-business profile would remove re-entry, which is the main friction ReserveCalifornia shows.
- Parks will still want their own editable copy, since staff fix rig details at check-in (Campspot, Firefly, Big Rig Media).

**Gaps:**

- KOA Rewards profile fields (saved rig or not) were not found. KOA help pages did not surface.
- Hipcamp's saved vehicle or profile features were not confirmed.
- How Campspot Marketplace shares the guest's saved RV with each park was not confirmed beyond the global profile statement.

## Operator concerns: owning the guest list, marketplace fees, marketing

**Takeaway:**
Operators pay 10 to 15 percent to marketplaces, and the selling point vendors use is that parks keep guest data. Direct booking and email to past guests are the stated way to cut those fees. A shared profile must therefore not look like a platform taking the guest relationship away from the park.

**Cited Findings:**

- Campspot Marketplace charges parks a 10% commission and says parks "automatically obtain and retain all customer data from Marketplace reservations" — [Campspot Marketplace](https://software.campspot.com/campspot-marketplace/) (vendor marketing, search summary)
- Campspot lists a 3% online booking fee paid by the guest, and says campgrounds cannot discount or waive the service fee — [Campspot pricing](https://software.campspot.com/pricing-new); [Campspot service fee](https://campspot.com/about/service-fee) (search summary)
- Firefly describes Campspot as 3% on all bookings plus a 10% marketplace fee — [Firefly vs Campspot](https://fireflyreservations.com/blog/firefly-reservations-vs.-campspot-which-is-right-for-your-park) (competitor, vendor marketing). ResNexus also cites 10% — [ResNexus: Switch from Campspot](https://resnexus.com/Comparisons/Campspot.html) (competitor, vendor marketing). These conflict on the 3% part.
- Hipcamp US charges private landowners 15% and PMS-integrated parks 12.5% — [Hipcamp: How much does it cost to list](https://support.hipcamp.com/hc/en-us/articles/360024823412-How-much-does-it-cost-to-list) (search summary). Hipcamp UK states 12.5% plus VAT — [Hipcamp UK](https://support.hipcamp.com/hc/en-gb/articles/360024823412-How-much-does-it-cost-to-list-and-is-there-a-commission-fee) (search summary). Its holiday park page says 15% — [Hipcamp operators AU](https://host.hipcamp.com/operators-au)
- Hipcamp integrates with Newbook as a PMS channel — [Hipcamp + Newbook](https://host.hipcamp.com/newbook)
- Campspot and DirectIQ sell syncing guest data into email lists to drive direct bookings — [Campspot marketing tools](https://software.campspot.com/campground-marketing-tools/); [DirectIQ for campgrounds](https://directiq.com/email-marketing-for-campgrounds) (vendor marketing)
- Campspot integrates guest data with Revinate and Iterable for marketing — [Revinate + Campspot](https://www.revinate.com/partner/campspot/); [Campspot Iterable FAQ](https://support.campspot.com/iterable-integration-overview-setup-faq) (vendor marketing)
- RoverPass reports OTA bookings fell to 14% of campground bookings in 2024 — [RoverPass blog](https://roverpass.com/blog/?p=11731) (vendor marketing)
- Booking Experts says OTAs limit operator access to guest data — [Booking Experts blog](https://www.bookingexperts.com/blogs/en/blogs/4-marketing-tips-to-boost-your-direct-bookings-and-reduce-ota-reliance-this-summer) (vendor marketing)
- RMS markets campground reservation software marketing features — [RMS blog](https://americas.rmscloud.com/blog/campground-reservation-software-marketing-capabilities) (vendor marketing)

**Inferences:**

- Parks will accept a camper-owned profile if each booking still gives the park a full copy it owns and can market to (with consent). That matches what Campspot promises its parks.
- A shared profile that looks like a marketplace (cross-park discovery) would raise the same fears as Hipcamp and Campspot Marketplace fees.

**Gaps:**

- No independent operator survey or ARVC / OHI primary data on guest data ownership was read. Operator forum views were not found in the searches run.

## Equivalent profile data in fitness and salons

**Takeaway:**
Fitness and salon profiles share contact, emergency contact, and consent records with campgrounds. They differ in the sensitive core: health and injury data for fitness, allergies and patch tests for salons, rigs and vehicles for camping. One profile model fits if it has a common core plus vertical-specific sections, with sensitive sections shared only by explicit camper consent.

**Cited Findings:**

- Mindbody's privacy policy lists contact details, "which may include third party emergency contact information" — [Mindbody privacy policy](https://www.mindbodyonline.com/privacy-policy.)
- Mindbody profiles can have custom profile fields, and form tools map to Emergency Contact Name, Relationship, and Phone — [LoopSpark: Mindbody profile field mapping](https://docs.loopspark.com/forms/form_mindbody_profile_updates); [WaiverElectronic Mindbody integration](https://www.waiverelectronic.com/MINDBODY-integration)
- Waiver tools create or update a Mindbody client profile and attach the signed PDF to it — [WaiverMaster help](https://help.waivermaster.com/?p=271); [Smartwaiver + Mindbody](https://www.smartwaiver.com/blog/announcing-the-smartwaiver-mindbody-integration)
- A studio vendor says Mindbody does not prompt clients to update their info unless staff send them to their profile, so data goes stale — [Studio integrations for Mindbody](https://studio-integrations.pages.dev/for-mindbody-studios) (vendor marketing)
- Vagaro client profiles reportedly track service history, color formulas, purchases, notes, and SOAP notes — [Agiled: client management software for beauty](https://agiled.app/blog/best-client-management-software-for-beauty-wellness) (third-party blog)
- A salon software guide calls allergy notes critical, with PPD sensitivity as the example, and describes patch test records as dated, since many color lines require one before a first chemical service — [Salon client management guide](https://salon-management-software.foretheta.workers.dev/blog/how-to-manage-salon-clients-software) (third-party blog)
- Spa and wellness intake covers health history, treatment preferences, contraindications, allergies, emergency contact, and medical disclosures — [Waiver King: Mindbody waivers](https://www.waiverking.com/blog/online-waiver-software-mindbodyonline-waiverking) (vendor marketing)

**Inferences:**

- Common core across all three verticals: name, contact, emergency contact, communication consent.
- Camper-owned, vertical-specific: rigs, vehicles, pets (camping); injuries, health notes (fitness); allergies, patch test dates (salons). Allergies and injuries are health data, so sharing rules must be stricter than for a rig.
- Business-owned in all three: internal notes, ratings and bans, waivers signed to that business, credits and memberships, service records such as color formulas (a stylist's work product), and site assignment.
- Waivers do not transfer. Each one is signed to a specific business and document version, in every vertical.
- Campground data is mostly non-sensitive equipment facts, which makes camping a lower-risk first test for cross-business sharing than fitness health data.

**Gaps:**

- Primary vendor docs for salon formula and allergy fields (Vagaro, Boulevard) were not read; findings rest on third-party blogs.
- No legal source was read on whether fitness injury or salon allergy data counts as health data under US state privacy laws.
