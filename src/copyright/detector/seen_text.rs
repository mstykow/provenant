use std::collections::HashSet;

use crate::copyright::types::{AuthorDetection, CopyrightDetection, HolderDetection};

pub(crate) struct SeenTextSets {
    pub(crate) copyrights: HashSet<String>,
    pub(crate) holders: HashSet<String>,
    pub(crate) authors: HashSet<String>,
}

impl SeenTextSets {
    pub(crate) fn from_existing(
        copyrights: &[CopyrightDetection],
        holders: &[HolderDetection],
        authors: &[AuthorDetection],
    ) -> Self {
        Self {
            copyrights: copyrights.iter().map(|c| c.copyright.clone()).collect(),
            holders: holders.iter().map(|h| h.holder.clone()).collect(),
            authors: authors.iter().map(|a| a.author.clone()).collect(),
        }
    }

    pub(crate) fn dedup_new_copyrights(
        &mut self,
        copyrights: &mut Vec<CopyrightDetection>,
        before: usize,
    ) {
        self.dedup_appended_copyrights(copyrights, before);
    }

    pub(crate) fn dedup_appended_copyrights(
        &mut self,
        copyrights: &mut Vec<CopyrightDetection>,
        before: usize,
    ) {
        if copyrights.len() <= before {
            return;
        }
        let mut i = before;
        while i < copyrights.len() {
            let text = &copyrights[i].copyright;
            if self.copyrights.insert(text.clone()) {
                i += 1;
            } else {
                copyrights.remove(i);
            }
        }
    }

    pub(crate) fn dedup_new_holders(&mut self, holders: &mut Vec<HolderDetection>, before: usize) {
        self.dedup_appended_holders(holders, before);
    }

    pub(crate) fn dedup_appended_holders(
        &mut self,
        holders: &mut Vec<HolderDetection>,
        before: usize,
    ) {
        if holders.len() <= before {
            return;
        }
        let mut i = before;
        while i < holders.len() {
            let text = &holders[i].holder;
            if self.holders.insert(text.clone()) {
                i += 1;
            } else {
                holders.remove(i);
            }
        }
    }

    pub(crate) fn dedup_new_authors(&mut self, authors: &mut Vec<AuthorDetection>, before: usize) {
        self.dedup_appended_authors(authors, before);
    }

    pub(crate) fn dedup_appended_authors(
        &mut self,
        authors: &mut Vec<AuthorDetection>,
        before: usize,
    ) {
        if authors.len() <= before {
            return;
        }
        let mut i = before;
        while i < authors.len() {
            if self.authors.insert(authors[i].author.clone()) {
                i += 1;
            } else {
                authors.remove(i);
            }
        }
    }

    pub(crate) fn register_copyrights(&mut self, copyrights: &[CopyrightDetection]) {
        for c in copyrights {
            self.copyrights.insert(c.copyright.clone());
        }
    }

    pub(crate) fn register_holders(&mut self, holders: &[HolderDetection]) {
        for h in holders {
            self.holders.insert(h.holder.clone());
        }
    }

    pub(crate) fn register_authors(&mut self, authors: &[AuthorDetection]) {
        for a in authors {
            self.authors.insert(a.author.clone());
        }
    }

    pub(crate) fn rebuild_copyrights_from(&mut self, copyrights: &[CopyrightDetection]) {
        self.copyrights = copyrights.iter().map(|c| c.copyright.clone()).collect();
    }

    pub(crate) fn rebuild_holders_from(&mut self, holders: &[HolderDetection]) {
        self.holders = holders.iter().map(|h| h.holder.clone()).collect();
    }

    pub(crate) fn rebuild_authors_from(&mut self, authors: &[AuthorDetection]) {
        self.authors = authors.iter().map(|a| a.author.clone()).collect();
    }
}
