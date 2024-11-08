FROM ailets-pymodule

USER root

RUN useradd -m -r ailets \
    && chown -R ailets:ailets /app

COPY ailets0.py /app/
RUN chmod +x /app/ailets0.py \
    && sed -i '/import localsetup/d' /app/ailets0.py \
    && ln -s /app/ailets0.py /usr/bin/ailets

USER ailets
ENV USER=ailets

ENTRYPOINT ["ailets"]
