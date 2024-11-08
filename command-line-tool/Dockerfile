FROM ailets-pymodule

COPY ailets0.py /app/
RUN chmod +x /app/ailets0.py \
    && sed -i '/import localsetup/d' /app/ailets0.py \
    && ln -s /app/ailets0.py /usr/bin/ailets

ENTRYPOINT ["ailets"] 