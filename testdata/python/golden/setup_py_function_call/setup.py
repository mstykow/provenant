from setuptools import setup

setup_args = {
    "name": "coverage",
    "version": "3.7",
    "license": "BSD",
    "description": "Code coverage measurement for Python",
    "url": "https://github.com/nedbat/coveragepy",
    "author": "Ned Batchelder and others",
    "author_email": "ned@nedbatchelder.com",
}


def main():
    setup(**setup_args)


if __name__ == "__main__":
    main()
